//! `aristo auth {login, status, token, logout}` — credential lifecycle.
//!
//! The handlers are thin: token resolution and persistence live in
//! `aristo_core::auth` so other binaries can call them directly.
//!
//! ## Login flow
//!
//! `aristo auth login --server <url>` is the GitHub OAuth flow: the CLI
//! fetches the authorize URL from the org's server, the user pastes the
//! code shown on the callback page, the server mints an `arta_*` token
//! — an org grant, valid for every repo the org admits the user to —
//! and the CLI stores it, one entry per server. CI and scripts read
//! `ARETTA_TOKEN` + `ARETTA_API_URL` from the environment and never
//! touch the store.

use aristo_core::auth::{
    self, login_command, login_server, AuthError, CredentialStore, ServerUrl, Token, UpsertOutcome,
};

use crate::{AuthAction, CliError, CliResult};

/// Dispatcher for `aristo auth` subcommands.
pub(crate) fn run(action: AuthAction) -> CliResult<()> {
    match action {
        AuthAction::Login { server } => login(server),
        AuthAction::Status => status(),
        AuthAction::Token { server } => token(server),
        AuthAction::Logout { all, server } => logout(all, server),
    }
}

// ─── login ─────────────────────────────────────────────────────────────────

fn login(server_flag: Option<String>) -> CliResult<()> {
    // The server the token is minted against is required (`--server`,
    // else `ARETTA_API_URL`): the platform apex cannot mint an org
    // grant, so it is never guessed.
    let env_override = std::env::var(auth::SERVER_ENV_VAR).ok();
    let (server, source) = login_server(server_flag.as_deref(), env_override.as_deref())
        .ok_or_else(|| CliError::Other {
            message: format!(
                "no server given.\n  \
                 Pass `--server https://<org>.aretta.ai` (your Aretta dashboard's hostname), \
                 or set {}.",
                auth::SERVER_ENV_VAR
            ),
            exit_code: 2,
        })?;

    // 1. Fetch the GitHub OAuth URL from the server.
    let init = auth::oauth_start(&server).map_err(auth_error_to_cli)?;

    // 2. Show the URL + try to open the browser. Name where the server
    //    came from, so a stale ARETTA_API_URL export is visible before
    //    the user authorizes.
    eprintln!();
    eprintln!("Authenticating against {server} ({})", source.provenance());
    eprintln!();
    eprintln!("Open this URL to authorize with GitHub:");
    eprintln!();
    eprintln!("    {}", init.authorize_url);
    eprintln!();
    let _ = try_open_browser(&init.authorize_url);
    eprintln!("After authorizing, the page will display a code. Paste it here:");

    // 3. Read the code from stdin (one line).
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(CliError::Io)?;
    let code = line.trim();
    if code.is_empty() {
        return Err(CliError::Other {
            message: format!(
                "no OAuth code provided. Re-run `{}` and paste the code from the callback page.",
                login_command()
            ),
            exit_code: 2,
        });
    }

    // 4. Exchange the code for an arta_* token. The checkout's repo, when
    //    the cwd is one, rides along as information only.
    let repo_hint = std::env::current_dir()
        .ok()
        .and_then(|d| auth::derive_repo_full_name(&d).ok());
    let resp = auth::oauth_exchange(&server, code, repo_hint.as_deref(), Some("aristo-cli"))
        .map_err(auth_error_to_cli)?;

    // 5. Persist the credential (token + server + user) and say what
    //    happened to the store, from the store as saved.
    let creds = aristo_core::auth::CredentialsRecord {
        token: Token::new(&resp.arta_token),
        server: server.clone(),
        user_login: Some(resp.user.login.clone()),
        user_id: Some(resp.user.id),
    };
    let report = aristo_core::auth::save_full(&creds).map_err(CliError::Io)?;
    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    println!("ok: authenticated as {} at {server}", resp.user.login);
    println!("    token saved to {}", path.display());
    println!(
        "    {}",
        store_change_line(report.outcome, &server, report.store.len())
    );
    if report.store.len() > 1 {
        println!(
            "    note: several servers on file — commands use the one named by --server or {}.",
            auth::SERVER_ENV_VAR
        );
    }
    if env_token_set() {
        println!(
            "    note: {} is set in the environment; it takes precedence over the saved entry.",
            auth::ENV_VAR
        );
    }
    println!("    `aristo auth status` to verify; `aristo auth logout --server <url>` to remove.");
    Ok(())
}

/// `entry added: server … — N server(s) on file.` (or `replaced`).
fn store_change_line(outcome: UpsertOutcome, server: &ServerUrl, total: usize) -> String {
    let what = match outcome {
        UpsertOutcome::Added => "entry added",
        UpsertOutcome::Replaced => "entry replaced",
    };
    format!(
        "{what}: server {server} — {total} {} on file.",
        if total == 1 { "server" } else { "servers" }
    )
}

/// The server a subcommand should act on: `--server`, else
/// `ARETTA_API_URL`, else none named.
fn server_flag_or_env(flag: Option<String>) -> Option<ServerUrl> {
    flag.map(|f| ServerUrl::parse(&f)).or_else(|| {
        std::env::var(auth::SERVER_ENV_VAR)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| ServerUrl::parse(&v))
    })
}

fn env_token_set() -> bool {
    std::env::var(auth::ENV_VAR).is_ok_and(|v| !v.trim().is_empty())
}

fn try_open_browser(url: &str) -> std::io::Result<()> {
    // Test mode: e2e tests spawn the real aristo binary and would
    // otherwise launch the developer's browser on every test run.
    // `ARISTO_NO_BROWSER` suppresses the spawn.
    if std::env::var("ARISTO_NO_BROWSER").is_ok() {
        return Ok(());
    }
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

fn auth_error_to_cli(e: AuthError) -> CliError {
    CliError::Other {
        message: e.to_string(),
        exit_code: 1,
    }
}

/// Map a store-load error into a CLI error with a recovery hint. The
/// store loader only ever returns `Malformed` (empty is `Ok`), but any
/// other variant is mapped defensively.
fn store_error_to_cli(e: AuthError) -> CliError {
    match e {
        AuthError::Malformed(msg) => CliError::Other {
            message: format!(
                "credentials file is malformed: {msg}\n  \
                 Run `aristo auth logout --all`, then `{}` to re-create it.",
                login_command()
            ),
            exit_code: 1,
        },
        other => auth_error_to_cli(other),
    }
}

/// A trailing note when `ARETTA_TOKEN` is set — it overrides the store,
/// so removing entries doesn't stop canon calls from using it.
fn note_env_still_set() {
    if env_token_set() {
        println!(
            "    note: {} is set in the environment; canon calls will still use it.",
            auth::ENV_VAR
        );
    }
}

// ─── status ────────────────────────────────────────────────────────────────

/// Which stored entry commands will use, given the store and the server
/// named by `ARETTA_API_URL` (if any). One line; the remedy when none.
fn selection_line(store: &CredentialStore, named: Option<&ServerUrl>) -> String {
    match (store.resolve_for(named), named) {
        (Some(e), Some(_)) => format!(
            "commands use: server {} (named by {}).",
            e.server,
            auth::SERVER_ENV_VAR
        ),
        (Some(e), None) => format!("commands use: server {}.", e.server),
        (None, Some(s)) => format!(
            "commands use: nothing — {} names {s}, which is not on file; run `aristo auth login --server {s}`.",
            auth::SERVER_ENV_VAR
        ),
        (None, None) => format!(
            "commands use: nothing — several servers on file; pass `--server <url>` or set {}.",
            auth::SERVER_ENV_VAR
        ),
    }
}

fn status() -> CliResult<()> {
    // Exit code mirrors the verdict: 0 iff a run from here would
    // authenticate (env pair, or a stored entry that resolves).
    // Everything printed is token-free.
    let env_server = std::env::var(auth::SERVER_ENV_VAR)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|v| ServerUrl::parse(&v));
    let store = auth::load_store().map_err(store_error_to_cli)?;
    let path = auth::credentials_path().map_err(auth_error_to_cli)?;

    let resolves = if env_token_set() {
        match &env_server {
            Some(server) => {
                println!(
                    "ok: signed in via {} for {server} ({} takes precedence over every stored entry).",
                    auth::ENV_VAR,
                    auth::ENV_VAR
                );
                true
            }
            None => {
                println!("not authenticated: {}", AuthError::EnvTokenWithoutServer);
                false
            }
        }
    } else if store.is_empty() {
        println!("{}", AuthError::NoToken);
        false
    } else {
        let picked = store.resolve_for(env_server.as_ref());
        println!(
            "{}: {} server(s) in {}",
            if picked.is_some() {
                "ok: signed in"
            } else {
                "signed in, but nothing selected"
            },
            store.len(),
            path.display()
        );
        for e in &store.entries {
            println!("    • {}", e.summary());
        }
        println!("    {}", selection_line(&store, env_server.as_ref()));
        picked.is_some()
    };

    if env_token_set() && !store.is_empty() {
        println!(
            "    also stored (shadowed by {}): {} server(s) in {}",
            auth::ENV_VAR,
            store.len(),
            path.display()
        );
    }
    if resolves {
        Ok(())
    } else {
        Err(CliError::Silent { exit_code: 1 })
    }
}

// ─── token ─────────────────────────────────────────────────────────────────

/// Print the resolved token to stdout — and NOTHING else — so it pipes
/// cleanly into a clipboard tool or a CI secret. Unlike `status`, this
/// deliberately prints the secret value, so it's only ever written to
/// stdout on explicit request.
fn token(server_flag: Option<String>) -> CliResult<()> {
    // Env var wins outright (CI precedence), like the resolver.
    if let Ok(v) = std::env::var(auth::ENV_VAR) {
        let v = v.trim();
        if !v.is_empty() {
            println!("{v}");
            return Ok(());
        }
    }
    let store = auth::load_store().map_err(store_error_to_cli)?;
    if store.is_empty() {
        return Err(CliError::Other {
            message: AuthError::NoToken.to_string(),
            exit_code: 1,
        });
    }
    let named = server_flag_or_env(server_flag);
    match store.resolve_for(named.as_ref()) {
        Some(e) => {
            println!("{}", e.token.as_str());
            Ok(())
        }
        None => Err(CliError::Other {
            message: match named {
                Some(s) => format!(
                    "no credential for {s}; run `aristo auth login --server {s}` \
                     (or `aristo auth status` to list what's stored)."
                ),
                None => "several servers on file — pass `--server <url>` to pick one \
                         (or `aristo auth status` to list)."
                    .into(),
            },
            exit_code: 1,
        }),
    }
}

// ─── logout ────────────────────────────────────────────────────────────────

fn logout(all: bool, server_flag: Option<String>) -> CliResult<()> {
    let path = auth::credentials_path().map_err(auth_error_to_cli)?;

    // `--all`: remove the whole file. Works even on a corrupt file.
    if all {
        let existed = path.exists();
        auth::clear().map_err(CliError::Io)?;
        if existed {
            println!(
                "ok: logged out. all credentials cleared from {}",
                path.display()
            );
        } else {
            println!("ok: not logged in (no credentials to clear).");
        }
        note_env_still_set();
        return Ok(());
    }

    let mut store = auth::load_store().map_err(|e| match e {
        AuthError::Malformed(msg) => CliError::Other {
            message: format!(
                "credentials file is malformed: {msg}\n  \
                 Run `aristo auth logout --all` to reset it."
            ),
            exit_code: 1,
        },
        other => auth_error_to_cli(other),
    })?;
    if store.is_empty() {
        println!("ok: not logged in (no credentials to clear).");
        note_env_still_set();
        return Ok(());
    }

    // Which entry? The same rule the resolver uses: the named server,
    // else the single entry.
    let named = server_flag_or_env(server_flag);
    let Some(server) = store.resolve_for(named.as_ref()).map(|e| e.server.clone()) else {
        return Err(CliError::Other {
            message: match named {
                Some(s) => format!("ok: no credential for {s} to remove (nothing changed)."),
                None => "several servers on file — pass `--server <url>` to log out of one, \
                         or `--all` to clear everything."
                    .into(),
            },
            exit_code: 2,
        });
    };
    store.remove_by_server(&server);

    // Persist: drop the file when the store is now empty, else rewrite it.
    if store.is_empty() {
        auth::clear().map_err(CliError::Io)?;
    } else {
        auth::save_store(&store).map_err(CliError::Io)?;
    }
    println!("ok: logged out of {server}. updated {}", path.display());
    note_env_still_set();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::auth::CredentialEntry;

    fn entry(server: &str, token: &str) -> CredentialEntry {
        CredentialEntry::bare(Token::new(token), ServerUrl::parse(server))
    }

    #[test]
    fn store_change_line_names_outcome_server_and_count() {
        let acme = ServerUrl::parse("https://acme.aretta.ai");
        assert_eq!(
            store_change_line(UpsertOutcome::Added, &acme, 1),
            "entry added: server https://acme.aretta.ai — 1 server on file."
        );
        assert_eq!(
            store_change_line(UpsertOutcome::Replaced, &acme, 2),
            "entry replaced: server https://acme.aretta.ai — 2 servers on file."
        );
    }

    #[test]
    fn selection_line_covers_the_four_cases() {
        let acme = ServerUrl::parse("https://acme.aretta.ai");
        let other = ServerUrl::parse("https://other.aretta.ai");
        let one = CredentialStore {
            entries: vec![entry("https://acme.aretta.ai", "a")],
        };
        assert_eq!(
            selection_line(&one, None),
            "commands use: server https://acme.aretta.ai."
        );
        assert!(selection_line(&one, Some(&acme)).contains("named by ARETTA_API_URL"));
        let v = selection_line(&one, Some(&other));
        assert!(
            v.contains("not on file") && v.contains("--server https://other.aretta.ai"),
            "{v}"
        );
        let two = CredentialStore {
            entries: vec![
                entry("https://acme.aretta.ai", "a"),
                entry("https://other.aretta.ai", "b"),
            ],
        };
        let v = selection_line(&two, None);
        assert!(
            v.contains("several servers") && v.contains("--server <url>"),
            "{v}"
        );
        assert!(!v.contains("a\"") && !v.contains(" b"), "no token: {v}");
    }
}
