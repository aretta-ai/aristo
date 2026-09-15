//! `aristo auth {login, status, logout}` — credential lifecycle.
//!
//! Wires `aristo_core::canon::auth` into the CLI dispatcher. The
//! handlers are intentionally thin: token-resolution and
//! persistence live in the library so other binaries (eventually a
//! `aretta-admin` clone or scripted tooling) can call them
//! directly without going through the CLI.
//!
//! ## Login flow
//!
//! `aristo auth login` is the GitHub OAuth flow and nothing else: the
//! CLI fetches the authorize URL from the server, the user pastes the
//! code shown on the callback page, the server mints an `arta_*` token
//! scoped to `(user, repo)`, and the CLI stores it keyed by server and
//! repo. There is no raw-token paste mode — CI and scripts read
//! `ARETTA_TOKEN` from the environment and never touch the store.

use std::path::Path;

use aristo_core::auth::{
    self, derive_repo_full_name, login_server, AuthError, CredentialEntry, CredentialStore,
    LoginServerSource, ServerUrl, Token, UpsertOutcome, UpsertReport,
};

use crate::{AuthAction, CliError, CliResult};

/// Dispatcher for `aristo auth` subcommands.
pub(crate) fn run(action: AuthAction) -> CliResult<()> {
    match action {
        AuthAction::Login { server, repo } => login(server, repo),
        AuthAction::Status => status(),
        AuthAction::Token { repo } => token(repo),
        AuthAction::Logout { all, repo } => logout(all, repo),
    }
}

// ─── login ─────────────────────────────────────────────────────────────────

fn login(server_flag: Option<String>, repo_flag: Option<String>) -> CliResult<()> {
    // The two things a token is scoped by, both required: the server
    // it is minted against (`--server`, else `ARETTA_API_URL`) and the
    // repo (`--repo`, else the checkout's origin). There is no default
    // server — the platform apex cannot mint an org token.
    let env_override = std::env::var("ARETTA_API_URL").ok();
    let (server, source) = login_server(server_flag.as_deref(), env_override.as_deref())
        .ok_or_else(|| CliError::Other {
            message: "no server given.\n  \
                      Pass `--server https://<org>.aretta.ai` (your org's Aretta host), \
                      or set ARETTA_API_URL."
                .into(),
            exit_code: 2,
        })?;
    let repo_full_name = resolve_repo_full_name(repo_flag)?;
    login_via_oauth(&server, source, repo_full_name)
}

fn login_via_oauth(
    server: &ServerUrl,
    source: LoginServerSource,
    repo_full_name: String,
) -> CliResult<()> {
    // 1. Fetch the GitHub OAuth URL from the proxy.
    let init = auth::oauth_start(server).map_err(auth_error_to_cli)?;

    // 2. Show the URL + try to open the browser. Name where the server
    //    came from, so a stale ARETTA_API_URL export is visible before
    //    the user authorizes.
    eprintln!();
    eprintln!("Authenticating against {server} ({})", source.provenance());
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
        (Err(why), _) => format!(
            "this directory is not a GitHub checkout ({why}); nothing resolves here — {remedy}"
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
        (Err(why), _) => format!(
            "this directory is not a GitHub checkout ({why}) — resolves to: no stored \
             credential ({} on file; run from a checkout of one of them, or set ARETTA_TOKEN).",
            store.len()
        ),
    }
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
/// entry for `--repo` (or the cwd's repo); nothing else.
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
    // `--repo` or the cwd's repo, matched strictly — asking for a repo
    // you're not logged in to errors rather than silently handing back
    // a different repo's token.
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
        cwd_repo.as_deref().and_then(|r| store.find_by_repo(r))
    };
    match entry {
        Some(e) => {
            println!("{}", e.token.as_str());
            Ok(())
        }
        None => Err(CliError::Other {
            message: "no credential for this checkout — pass `--repo <owner/repo>` to pick one \
                      (or `aristo auth status` to list what's stored)."
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

    // Which entry? `--repo`, else the cwd's repo. Nothing else — the
    // same rule the resolver uses to pick an entry.
    let Some(repo) = resolve_repo_best_effort(repo_flag)? else {
        return Err(CliError::Other {
            message: "not a GitHub checkout — pass `--repo <owner/repo>` to log out of one \
                      credential, or `--all` to clear everything."
                .into(),
            exit_code: 2,
        });
    };
    if store.remove_by_repo(&repo) == 0 {
        println!("ok: no credential for {repo} to remove (nothing changed).");
        note_env_still_set();
        return Ok(());
    }
    let removed_label = format!("of {repo}");

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
        let unscoped = entry("https://code.aretta.ai", None, "t");
        assert!(store_change_line(UpsertOutcome::Added, &unscoped, 1).contains("repo (unscoped)"));
    }

    #[test]
    fn login_verdict_matching_checkout_resolves() {
        let tmp = TempDir::new().unwrap();
        let dir = checkout(tmp.path(), "w", "acme/widgets");
        let saved = entry("https://code.aretta.ai", Some("acme/widgets"), "t1");
        let st = store(vec![
            entry("https://code.aretta.ai", Some("other/x"), "t0"),
            saved.clone(),
        ]);
        assert_eq!(
            login_verdict(&st, &saved, &dir),
            "this checkout (acme/widgets) resolves to this entry."
        );
    }

    #[test]
    fn login_verdict_mismatched_checkout_names_the_repo_and_both_remedies() {
        let tmp = TempDir::new().unwrap();
        let dir = checkout(tmp.path(), "fork", "alice/widgets");
        let saved = entry("https://code.aretta.ai", Some("acme/widgets"), "t1");
        let st = store(vec![
            entry("https://code.aretta.ai", Some("other/x"), "t0"),
            saved.clone(),
        ]);
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
    fn login_verdict_outside_a_checkout_never_resolves() {
        // No single-entry fallback: even the only credential on file does
        // not apply in a directory that is not its checkout.
        let tmp = TempDir::new().unwrap();
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let saved = entry("https://code.aretta.ai", Some("acme/widgets"), "t1");
        let one = store(vec![saved.clone()]);
        let v = login_verdict(&one, &saved, &plain);
        assert!(v.contains("not a GitHub checkout (no .git/config"), "{v}");
        assert!(v.contains("nothing resolves here"), "{v}");
        assert!(v.contains("run aristo from a acme/widgets checkout"), "{v}");
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
        let two = store(vec![
            entry("https://code.aretta.ai", Some("other/x"), "t0"),
            a.clone(),
        ]);
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
        // The only entry on file does not apply outside its checkout either.
        let one = store(vec![a]);
        let v = status_verdict(&one, &plain);
        assert!(v.contains("no stored credential (1 on file"), "{v}");
    }
}
