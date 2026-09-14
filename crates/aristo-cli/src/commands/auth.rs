//! `aristo auth {login, status, logout}` — credential lifecycle.
//!
//! Wires `aristo_core::canon::auth` into the CLI dispatcher. The
//! handlers are intentionally thin: token-resolution and
//! persistence live in the library so other binaries (eventually a
//! `aretta-admin` clone or scripted tooling) can call them
//! directly without going through the CLI.
//!
//! ## Login flow (paste-flow, deliberately simple)
//!
//! The first slice of `aristo auth login` is a **paste flow**:
//!
//! 1. Print a one-line prompt telling the user where to get a token.
//! 2. Read a token from stdin (`--stdin` consumes all; default reads
//!    one line; `--token=<T>` bypasses both for tests / scripting).
//! 3. Persist via `canon::auth::save`.
//!
//! Server-side validation of the token (e.g., `GET /auth/whoami`) is
//! intentionally deferred — the first canon API call (`aristo stamp`,
//! `aristo critique`, `aristo canon show`) surfaces a typed
//! [`AuthError::Invalid`] if the token is bad. Adding a validation
//! roundtrip here would couple `aristo auth login` to network state,
//! breaking the offline-friendly invariant.
//!
//! A device-code OAuth flow is a future enhancement (open browser →
//! poll for token); not needed for v0.1.

use std::io::Read;
use std::path::Path;

use aristo_core::auth::{
    self, derive_repo_full_name, login_server, login_server_discovering, AuthError,
    CredentialEntry, CredentialStore, LoginServerSource, ServerUrl, Token, UpsertOutcome,
    UpsertReport,
};

use crate::{AuthAction, CliError, CliResult};

/// Dispatcher for `aristo auth` subcommands.
pub(crate) fn run(action: AuthAction) -> CliResult<()> {
    match action {
        AuthAction::Login {
            stdin,
            token,
            server,
            repo,
        } => login(stdin, token, server, repo),
        AuthAction::Status => status(),
        AuthAction::Token { repo } => token(repo),
        AuthAction::Logout { all, repo } => logout(all, repo),
    }
}

// ─── login ─────────────────────────────────────────────────────────────────

fn login(
    read_stdin: bool,
    token_flag: Option<String>,
    server_flag: Option<String>,
    repo_flag: Option<String>,
) -> CliResult<()> {
    // Bypass modes — caller supplied a raw token directly. No OAuth and
    // no discovery (the token's scope is already fixed server-side), but
    // `--server` / `--repo` still key the stored entry so the multi-repo
    // store can look it up later.
    if read_stdin || token_flag.is_some() {
        return login_with_raw_token(read_stdin, token_flag, server_flag, repo_flag);
    }

    // OAuth flow. Resolve the repo first — both zero-config discovery
    // and token scoping need it.
    let repo_full_name = resolve_repo_full_name(repo_flag)?;

    // Resolve the server the token is minted against. Precedence:
    // --server flag > ARETTA_API_URL env > zero-config org discovery
    // (queried at the platform) > the platform default. Discovery runs
    // only when neither flag nor env is supplied, so an explicit choice
    // always wins and skips the network lookup. Honoring the env keeps
    // the auth plane aligned with the data plane, which already treats
    // ARETTA_API_URL as its highest-precedence override (see
    // `crate::data_plane`).
    let env_override = std::env::var("ARETTA_API_URL").ok();
    let platform = discovery_platform();
    let (server, source) = login_server_discovering(
        server_flag.as_deref(),
        env_override.as_deref(),
        &platform,
        |p| auth::discover_org(p, &repo_full_name),
    );

    login_via_oauth(&server, source, repo_full_name)
}

/// The platform where zero-config org discovery is queried — and the
/// server login falls back to when discovery misses. Defaults to the
/// prod platform (`code.aretta.ai`); `ARETTA_DISCOVERY_URL` relocates it
/// for self-hosted deployments (and offline tests). Distinct from
/// `ARETTA_API_URL`, which pins the login server outright and skips
/// discovery entirely.
fn discovery_platform() -> ServerUrl {
    match std::env::var("ARETTA_DISCOVERY_URL").ok() {
        Some(v) if !v.trim().is_empty() => ServerUrl::parse(&v),
        _ => ServerUrl::Prod,
    }
}

fn login_via_oauth(
    server: &ServerUrl,
    source: LoginServerSource,
    repo_full_name: String,
) -> CliResult<()> {
    // 1. Fetch the GitHub OAuth URL from the proxy.
    let init = auth::oauth_start(server).map_err(auth_error_to_cli)?;

    // 2. Show the URL + try to open the browser. Name where the server
    //    came from when it wasn't the default, so a stale ARETTA_API_URL
    //    export is visible before the user authorizes.
    eprintln!();
    match source.provenance(&repo_full_name) {
        Some(prov) => eprintln!("Authenticating against {server} ({prov})"),
        None => eprintln!("Authenticating against {server}"),
    }
    eprintln!("Scoping token to repo: {repo_full_name}");
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
            message: "no OAuth code provided. Re-run `aristo auth login` and paste the code from the callback page.".into(),
            exit_code: 2,
        });
    }

    // 4. Exchange the code for an arta_* token.
    let resp = auth::oauth_exchange(server, code, &repo_full_name, Some("aristo-cli"))
        .map_err(auth_error_to_cli)?;

    // 5. Persist the full credentials record (token + server + user + repo).
    let token = Token::new(&resp.arta_token);
    let creds = aristo_core::auth::CredentialsRecord {
        token,
        server: server.clone(),
        user_login: Some(resp.user.login.clone()),
        user_id: Some(resp.user.id),
        repo: Some(resp.repo_full_name.clone()),
    };
    let report = aristo_core::auth::save_full(&creds).map_err(CliError::Io)?;

    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    println!(
        "ok: authenticated as {} for {}",
        resp.user.login, resp.repo_full_name
    );
    println!("    token saved to {}", path.display());
    print_login_report(&report, &creds.token)?;
    println!("    `aristo auth status` to verify; `aristo auth logout` to remove.");
    Ok(())
}

fn login_with_raw_token(
    read_stdin: bool,
    token_flag: Option<String>,
    server_flag: Option<String>,
    repo_flag: Option<String>,
) -> CliResult<()> {
    let token_raw = collect_raw_token(read_stdin, token_flag)?;
    let trimmed = token_raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::Other {
            message: "no token provided.\n\
                     Run `aristo auth login` (OAuth flow, default) to mint one interactively, or if you already have an arta_* token:\n  \
                       `aristo auth login --stdin` (pipe), or\n  \
                       `aristo auth login --token <TOKEN>` (scripting)."
                .into(),
            exit_code: 2,
        });
    }
    // Key the entry by (resolved server, repo). No discovery — the token
    // scope is already fixed server-side; we only record where it came
    // from so the multi-repo store can look it up. Server precedence is
    // --server > ARETTA_API_URL > prod; the repo is --repo or the cwd's
    // git remote (best-effort — absent is fine for a scriptless paste).
    let env_override = std::env::var("ARETTA_API_URL").ok();
    let (server, _) = login_server(server_flag.as_deref(), env_override.as_deref());
    let repo = resolve_repo_best_effort(repo_flag)?;
    let creds = aristo_core::auth::CredentialsRecord {
        token: Token::new(trimmed),
        server,
        user_login: None,
        user_id: None,
        repo,
    };
    let report = aristo_core::auth::save_full(&creds).map_err(CliError::Io)?;

    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    println!("ok: authenticated. token saved to {}", path.display());
    print_login_report(&report, &creds.token)?;
    println!("    `aristo auth status` to verify; `aristo auth logout` to remove.");
    Ok(())
}

// ─── what the store now holds, and what THIS directory resolves to ─────────

/// After a login: what happened to the store and whether the directory
/// the user is standing in will actually use the new entry. Both are
/// answered from the store as saved — no second read — and the
/// resolution verdict goes through the resolver's own selection rule.
fn print_login_report(report: &UpsertReport, saved_token: &Token) -> CliResult<()> {
    let saved = report
        .store
        .entries
        .iter()
        .find(|e| e.token.as_str() == saved_token.as_str())
        .ok_or_else(|| CliError::Other {
            message: "internal: the saved credential is not in the store just written".into(),
            exit_code: 1,
        })?;
    println!(
        "    {}",
        store_change_line(report.outcome, saved, report.store.len())
    );
    let cwd = std::env::current_dir().map_err(CliError::Io)?;
    println!("    {}", login_verdict(&report.store, saved, &cwd));
    if std::env::var(auth::ENV_VAR).is_ok_and(|v| !v.trim().is_empty()) {
        println!(
            "    note: {} is set in the environment; it takes precedence over the saved entry.",
            auth::ENV_VAR
        );
    }
    Ok(())
}

/// `entry added: server …, repo … — N entries on file.` (or `replaced`,
/// naming how many older entries for the repo were dropped).
fn store_change_line(outcome: UpsertOutcome, saved: &CredentialEntry, total: usize) -> String {
    let what = match outcome {
        UpsertOutcome::Added => "entry added".to_string(),
        UpsertOutcome::Replaced { dropped } => format!(
            "entry replaced (dropped {dropped} older {} for this repo)",
            plural(dropped, "entry", "entries")
        ),
    };
    format!(
        "{what}: {} — {total} {} on file.",
        entry_key(saved),
        plural(total, "entry", "entries")
    )
}

/// The `(server, repo)` key of an entry, as shown to the user.
fn entry_key(e: &CredentialEntry) -> String {
    format!(
        "server {}, repo {}",
        e.server,
        e.repo.as_deref().unwrap_or("(unscoped)")
    )
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { one } else { many }.to_string()
}

/// The entry the resolver will pick when run from `dir`, alongside the
/// derived checkout (or why none could be derived). One call site for
/// the rule, so login, status and the resolver agree.
fn resolution_at<'s>(
    store: &'s CredentialStore,
    dir: &Path,
) -> (Result<String, String>, Option<&'s CredentialEntry>) {
    let checkout = auth::checkout_at(dir);
    let picked = store.resolve_for(checkout.as_deref().ok());
    (checkout, picked)
}

/// Will running aristo from `dir` use `saved`? One line, with the
/// remedy when the answer is no.
fn login_verdict(store: &CredentialStore, saved: &CredentialEntry, dir: &Path) -> String {
    let (checkout, picked) = resolution_at(store, dir);
    let uses_saved = picked.is_some_and(|e| e.token.as_str() == saved.token.as_str());
    let remedy = match saved.repo.as_deref() {
        Some(repo) => format!(
            "run aristo from a {repo} checkout, or set ARETTA_TOKEN=$(aristo auth token --repo {repo})."
        ),
        None => "set ARETTA_TOKEN to use it.".to_string(),
    };
    match (checkout, uses_saved) {
        (Ok(repo), true) => format!("this checkout ({repo}) resolves to this entry."),
        (Ok(repo), false) => {
            format!("this checkout ({repo}) will NOT resolve to this entry — {remedy}")
        }
        (Err(why), true) => format!(
            "this directory is not a GitHub checkout ({why}); as the sole entry on file, \
             this entry still resolves here."
        ),
        (Err(why), false) => format!(
            "this directory is not a GitHub checkout ({why}); with {} entries on file \
             nothing resolves here — {remedy}",
            store.len()
        ),
    }
}

/// `aristo auth status`' view of the same question: which stored entry
/// a run from `dir` will use, or the one-line fix if none.
fn status_verdict(store: &CredentialStore, dir: &Path) -> String {
    let (checkout, picked) = resolution_at(store, dir);
    match (checkout, picked) {
        (Ok(repo), Some(e)) => format!("this checkout ({repo}) resolves to: {}", entry_key(e)),
        (Ok(repo), None) => format!(
            "this checkout ({repo}) resolves to: no stored credential — \
             run `aristo auth login --repo {repo}` here, or set ARETTA_TOKEN."
        ),
        (Err(why), Some(e)) => format!(
            "this directory is not a GitHub checkout ({why}) — resolves to: {} (the sole entry)",
            entry_key(e)
        ),
        (Err(why), None) => format!(
            "this directory is not a GitHub checkout ({why}) — resolves to: no stored \
             credential ({} on file; run from a checkout of one of them, or set ARETTA_TOKEN).",
            store.len()
        ),
    }
}

/// Determine where the raw token comes from in bypass modes.
fn collect_raw_token(read_stdin: bool, token_flag: Option<String>) -> CliResult<String> {
    if let Some(t) = token_flag {
        return Ok(t);
    }
    if read_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(CliError::Io)?;
        return Ok(buf);
    }
    // Should not be reached — caller checks the flags first.
    Err(CliError::Other {
        message: "internal: collect_raw_token called without --stdin or --token".into(),
        exit_code: 1,
    })
}

/// Validate a `--repo owner/repo` flag value.
fn validate_repo_flag(raw: &str) -> CliResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::Other {
            message: "--repo must be `owner/repo` (got empty string)".into(),
            exit_code: 2,
        });
    }
    if !trimmed.contains('/') {
        return Err(CliError::Other {
            message: format!("--repo `{trimmed}` is not in `owner/repo` form"),
            exit_code: 2,
        });
    }
    Ok(trimmed.to_string())
}

/// Resolve `owner/repo`, requiring one: the `--repo` flag, else the
/// cwd's git remote (erroring with a `--repo` hint if neither works).
fn resolve_repo_full_name(repo_flag: Option<String>) -> CliResult<String> {
    if let Some(r) = repo_flag {
        return validate_repo_flag(&r);
    }
    let cwd = std::env::current_dir().map_err(CliError::Io)?;
    derive_repo_full_name(&cwd).map_err(auth_error_to_cli)
}

/// Resolve `owner/repo` best-effort: the `--repo` flag (validated), else
/// the cwd's git remote, else `None`. Used where a missing repo is
/// acceptable (a raw-token paste, or a repo-scoped lookup that renders
/// its own "which repo?" error).
fn resolve_repo_best_effort(repo_flag: Option<String>) -> CliResult<Option<String>> {
    if let Some(r) = repo_flag {
        return Ok(Some(validate_repo_flag(&r)?));
    }
    Ok(std::env::current_dir()
        .ok()
        .and_then(|cwd| derive_repo_full_name(&cwd).ok()))
}

fn try_open_browser(url: &str) -> std::io::Result<()> {
    // Test mode: e2e tests spawn the real aristo binary and would
    // otherwise launch the developer's browser on every test run.
    // The `ARISTO_NO_BROWSER` env var suppresses the spawn. Set it
    // in tests + any CI that doesn't want browser pop-ups.
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
                 Run `aristo auth logout --all` then `aristo auth login` to re-create it."
            ),
            exit_code: 1,
        },
        other => auth_error_to_cli(other),
    }
}

/// A trailing note when `ARETTA_TOKEN` is set — it overrides the store,
/// so removing entries doesn't stop canon calls from using it.
fn note_env_still_set() {
    if std::env::var(auth::ENV_VAR).is_ok() {
        println!(
            "    note: {} is set in the environment; canon calls will still use it.",
            auth::ENV_VAR
        );
    }
}

// ─── status ────────────────────────────────────────────────────────────────

fn status() -> CliResult<()> {
    let env_set = std::env::var(auth::ENV_VAR)
        .ok()
        .is_some_and(|v| !v.trim().is_empty());
    if env_set {
        println!(
            "ok: authenticated via {} environment variable.",
            auth::ENV_VAR
        );
        println!("    (env var takes precedence over the on-disk credentials file.)");
    }

    // List every stored credential — never the token itself.
    let store = auth::load_store().map_err(store_error_to_cli)?;
    if store.is_empty() {
        if !env_set {
            println!("not authenticated.");
            println!(
                "    Run `aristo auth login` to log in, or set the {} env var for CI.",
                auth::ENV_VAR
            );
            // Not an error — CI gates on the stdout text, not the exit
            // code (unauthenticated must not fail the process).
        }
        return Ok(());
    }

    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    if env_set {
        println!(
            "    also stored (shadowed by {}): {} credential(s) in {}",
            auth::ENV_VAR,
            store.len(),
            path.display()
        );
    } else {
        println!(
            "ok: authenticated — {} credential(s) in {}",
            store.len(),
            path.display()
        );
    }
    for e in &store.entries {
        let repo = e.repo.as_deref().unwrap_or("(unscoped)");
        match &e.user_login {
            Some(user) => println!("    • server: {}   repo: {repo}   user: {user}", e.server),
            None => println!("    • server: {}   repo: {repo}", e.server),
        }
    }
    // The verdict for THIS directory — the question a user standing in
    // the wrong checkout actually has.
    if env_set {
        println!(
            "    this checkout: {} takes precedence over every stored entry.",
            auth::ENV_VAR
        );
    } else {
        let cwd = std::env::current_dir().map_err(CliError::Io)?;
        println!("    {}", status_verdict(&store, &cwd));
    }
    Ok(())
}

// ─── token ─────────────────────────────────────────────────────────────────

/// Print the resolved token to stdout — and NOTHING else — so it pipes
/// cleanly into a clipboard tool (`aristo auth token | pbcopy`) or a CI
/// secret. Unlike `status`, this deliberately prints the secret value, so
/// it's only ever written to stdout on explicit request. Resolves the
/// entry for `--repo` (or the cwd's repo), falling back to the sole
/// stored entry.
fn token(repo_flag: Option<String>) -> CliResult<()> {
    // Env var wins outright (CI precedence), like `resolve`.
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
            message: format!(
                "not authenticated — no token found.\n  \
                 Run `aristo auth login` to mint one, or set the {} env var.",
                auth::ENV_VAR
            ),
            exit_code: 1,
        });
    }
    // An explicit `--repo` must match strictly — no single-entry
    // fallback, so asking for a repo you're not logged in to errors
    // rather than silently handing back a different repo's token. With
    // no `--repo`, prefer the cwd's repo, else the sole stored entry.
    let entry = if let Some(raw) = repo_flag {
        let repo = validate_repo_flag(&raw)?;
        match store.find_by_repo(&repo) {
            Some(e) => Some(e),
            None => {
                return Err(CliError::Other {
                    message: format!(
                        "no credential for {repo}; run `aristo auth login --repo {repo}` \
                         (or `aristo auth status` to list what's stored)."
                    ),
                    exit_code: 1,
                })
            }
        }
    } else {
        let cwd_repo = std::env::current_dir()
            .ok()
            .and_then(|cwd| derive_repo_full_name(&cwd).ok());
        cwd_repo
            .as_deref()
            .and_then(|r| store.find_by_repo(r))
            .or_else(|| store.sole())
    };
    match entry {
        Some(e) => {
            println!("{}", e.token.as_str());
            Ok(())
        }
        None => Err(CliError::Other {
            message: "several credentials stored — pass `--repo <owner/repo>` to pick one \
                      (or `aristo auth status` to list)."
                .into(),
            exit_code: 1,
        }),
    }
}

// ─── logout ────────────────────────────────────────────────────────────────

fn logout(all: bool, repo_flag: Option<String>) -> CliResult<()> {
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

    // Which entry? `--repo` / the cwd repo; or the sole entry when that
    // is unambiguous (single-repo convenience).
    let repo_hint = resolve_repo_best_effort(repo_flag)?;
    let removed_label = match &repo_hint {
        Some(r) => {
            if store.remove_by_repo(r) == 0 {
                println!("ok: no credential for {r} to remove (nothing changed).");
                note_env_still_set();
                return Ok(());
            }
            format!("of {r}")
        }
        None => {
            if store.len() == 1 {
                store.entries.clear();
                "the stored credential".to_string()
            } else {
                return Err(CliError::Other {
                    message: "several credentials stored — pass `--repo <owner/repo>` to log out \
                              of one, or `--all` to clear everything."
                        .into(),
                    exit_code: 2,
                });
            }
        }
    };

    // Persist: drop the file when the store is now empty, else rewrite it.
    if store.is_empty() {
        auth::clear().map_err(CliError::Io)?;
    } else {
        auth::save_store(&store).map_err(CliError::Io)?;
    }
    println!("ok: logged out {removed_label}. updated {}", path.display());
    note_env_still_set();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn entry(server: &str, repo: Option<&str>, token: &str) -> CredentialEntry {
        CredentialEntry::bare(
            Token::new(token),
            ServerUrl::parse(server),
            repo.map(str::to_string),
        )
    }

    fn store(entries: Vec<CredentialEntry>) -> CredentialStore {
        CredentialStore { entries }
    }

    fn checkout(parent: &Path, name: &str, owner_repo: &str) -> std::path::PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            format!("[remote \"origin\"]\n    url = git@github.com:{owner_repo}.git\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn store_change_line_names_outcome_key_and_count() {
        let e = entry("https://acme.aretta.ai", Some("acme/widgets"), "t");
        assert_eq!(
            store_change_line(UpsertOutcome::Added, &e, 1),
            "entry added: server https://acme.aretta.ai, repo acme/widgets — 1 entry on file."
        );
        assert_eq!(
            store_change_line(UpsertOutcome::Replaced { dropped: 2 }, &e, 3),
            "entry replaced (dropped 2 older entries for this repo): server https://acme.aretta.ai, repo acme/widgets — 3 entries on file."
        );
        let unscoped = entry("prod", None, "t");
        assert!(store_change_line(UpsertOutcome::Added, &unscoped, 1).contains("repo (unscoped)"));
    }

    #[test]
    fn login_verdict_matching_checkout_resolves() {
        let tmp = TempDir::new().unwrap();
        let dir = checkout(tmp.path(), "w", "acme/widgets");
        let saved = entry("prod", Some("acme/widgets"), "t1");
        let st = store(vec![entry("prod", Some("other/x"), "t0"), saved.clone()]);
        assert_eq!(
            login_verdict(&st, &saved, &dir),
            "this checkout (acme/widgets) resolves to this entry."
        );
    }

    #[test]
    fn login_verdict_mismatched_checkout_names_the_repo_and_both_remedies() {
        let tmp = TempDir::new().unwrap();
        let dir = checkout(tmp.path(), "fork", "alice/widgets");
        let saved = entry("prod", Some("acme/widgets"), "t1");
        let st = store(vec![entry("prod", Some("other/x"), "t0"), saved.clone()]);
        let v = login_verdict(&st, &saved, &dir);
        assert!(
            v.starts_with("this checkout (alice/widgets) will NOT resolve to this entry"),
            "{v}"
        );
        assert!(v.contains("run aristo from a acme/widgets checkout"), "{v}");
        assert!(
            v.contains("ARETTA_TOKEN=$(aristo auth token --repo acme/widgets)"),
            "{v}"
        );
    }

    #[test]
    fn login_verdict_outside_a_checkout_depends_on_the_sole_entry_grace() {
        let tmp = TempDir::new().unwrap();
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let saved = entry("prod", Some("acme/widgets"), "t1");
        // Sole entry: still resolves.
        let one = store(vec![saved.clone()]);
        let v = login_verdict(&one, &saved, &plain);
        assert!(v.contains("not a GitHub checkout (no .git/config"), "{v}");
        assert!(v.contains("this entry still resolves here"), "{v}");
        // Several entries: nothing resolves; remedy given.
        let two = store(vec![entry("prod", Some("other/x"), "t0"), saved.clone()]);
        let v = login_verdict(&two, &saved, &plain);
        assert!(
            v.contains("with 2 entries on file nothing resolves here"),
            "{v}"
        );
        assert!(v.contains("ARETTA_TOKEN"), "{v}");
    }

    #[test]
    fn status_verdict_covers_all_four_branches() {
        let tmp = TempDir::new().unwrap();
        let acme = checkout(tmp.path(), "acme", "acme/widgets");
        let fork = checkout(tmp.path(), "fork", "alice/widgets");
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let a = entry("https://acme.aretta.ai", Some("acme/widgets"), "t1");
        let two = store(vec![entry("prod", Some("other/x"), "t0"), a.clone()]);
        assert_eq!(
            status_verdict(&two, &acme),
            "this checkout (acme/widgets) resolves to: server https://acme.aretta.ai, repo acme/widgets"
        );
        let v = status_verdict(&two, &fork);
        assert!(
            v.starts_with("this checkout (alice/widgets) resolves to: no stored credential"),
            "{v}"
        );
        assert!(
            v.contains("`aristo auth login --repo alice/widgets` here"),
            "{v}"
        );
        let v = status_verdict(&two, &plain);
        assert!(v.contains("not a GitHub checkout"), "{v}");
        assert!(v.contains("no stored credential (2 on file"), "{v}");
        let one = store(vec![a]);
        let v = status_verdict(&one, &plain);
        assert!(v.ends_with("repo acme/widgets (the sole entry)"), "{v}");
    }
}
