//! Auth-token resolution — env var → file → [`AuthError::NoToken`].
//!
//! Three sources, checked in order:
//!
//! 1. `ARETTA_TOKEN` env var — CI-friendly; takes precedence over
//!    the on-disk credentials file so `ARETTA_TOKEN=… cargo test`
//!    works without touching `~/.config/aristo/credentials`.
//! 2. Per-user credentials file under [`super::store::config_dir`].
//! 3. No token → [`AuthError::NoToken`]. The SDK surfaces "run
//!    `aristo auth login`" as the recovery hint.

use std::path::Path;

use super::error::AuthError;
use super::server::ServerUrl;
use super::store::{home_dir, load_store_with};
use super::token::Token;

/// Full resolved-credentials record. Returned by [`resolve_full`] for
/// callers that need the server URL + user identity alongside the
/// token. Plain [`resolve`] returns only the [`Token`] for callers
/// that don't.
#[derive(Debug, Clone)]
pub struct ResolvedCreds {
    pub token: Token,
    /// Aretta server this token was minted against. Defaults to
    /// [`ServerUrl::Prod`] when the source is `ARETTA_TOKEN` or an
    /// old bare-token file with no `server` field.
    pub server: ServerUrl,
    /// `Some(login)` when sourced from a credentials file with a
    /// recorded user. `None` for env-var source.
    pub user_login: Option<String>,
    /// Numeric GitHub user id at mint time.
    pub user_id: Option<u64>,
    /// Repo the token is scoped to server-side.
    pub repo: Option<String>,
}

/// Environment variable that overrides the on-disk credentials.
pub const ENV_VAR: &str = "ARETTA_TOKEN";

/// Resolve the auth token via the documented precedence.
///
/// Callers typically wrap the resolved token in an HTTP client across
/// calls; no need to re-resolve per call.
pub fn resolve() -> Result<Token, AuthError> {
    resolve_with(
        std::env::var(ENV_VAR).ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Resolve with explicit env-var and home-dir overrides. Tests use
/// this to avoid mutating process state (the workspace forbids
/// `unsafe_code`, which `std::env::set_var` requires). No repo hint —
/// resolves the env token, else the sole stored entry.
pub fn resolve_with(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<Token, AuthError> {
    Ok(resolve_full_with(env_token, xdg_config_home, home_override, None)?.token)
}

/// Like [`resolve`] but returns the full credentials record (server,
/// user, repo). Use this from canon / verify call sites that need the
/// server URL paired with the token. Prefers the entry scoped to the
/// current repo (derived best-effort from the cwd's `.git/config`),
/// falling back to the sole stored entry.
pub fn resolve_full() -> Result<ResolvedCreds, AuthError> {
    let checkout = cwd_checkout();
    resolve_full_for_checkout(
        std::env::var(ENV_VAR).ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
        checkout.as_deref().map_err(String::as_str),
    )
}

/// `owner/repo` derived from the current directory's git remote, or
/// the reason it could not be (not a git checkout, no `origin`,
/// non-GitHub URL). The reason travels into
/// [`AuthError::NoEntryForCheckout`] so the user sees why no entry was
/// matched rather than a bare "no token".
pub fn cwd_checkout() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    checkout_at(&cwd)
}

/// [`cwd_checkout`] for an explicit directory — the same derivation
/// and the same failure text, so a CLI verdict about "this checkout"
/// cannot disagree with what the resolver did.
pub fn checkout_at(dir: &Path) -> Result<String, String> {
    super::git::derive_repo_full_name(dir).map_err(|e| match e {
        AuthError::Malformed(why) => why,
        other => other.to_string(),
    })
}

/// Resolve a full credentials record with explicit overrides and a repo
/// hint. Precedence: `ARETTA_TOKEN` env > the entry scoped to
/// `repo_hint` > the sole stored entry (single-repo grace). `None` is
/// "no checkout given" — see [`resolve_full_for_checkout`] to carry the
/// reason a repo could not be derived.
pub fn resolve_full_with(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
    repo_hint: Option<&str>,
) -> Result<ResolvedCreds, AuthError> {
    resolve_full_for_checkout(
        env_token,
        xdg_config_home,
        home_override,
        repo_hint.ok_or("no checkout given"),
    )
}

/// Resolve for a checkout that is either `Ok(owner/repo)` or `Err(why
/// it could not be derived)`. Same precedence as [`resolve_full_with`];
/// the derivation error only ever surfaces inside
/// [`AuthError::NoEntryForCheckout`], when several entries are stored
/// and none can be picked for this directory.
pub fn resolve_full_for_checkout(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
    checkout: Result<&str, &str>,
) -> Result<ResolvedCreds, AuthError> {
    // 1. Env var first — CI-friendly precedence. No metadata
    //    available; default to Prod server, no user/repo.
    if let Some(t) = env_token {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(ResolvedCreds {
                token: Token::new(t),
                server: ServerUrl::Prod,
                user_login: None,
                user_id: None,
                repo: None,
            });
        }
    }
    // 2. On-disk store (reads v2, migrates v1 + bare-token transparently).
    let store = load_store_with(xdg_config_home, home_override)?;
    if store.is_empty() {
        return Err(AuthError::NoToken);
    }
    // 3. Prefer the entry scoped to the current repo; else fall back to
    //    the sole entry so a one-credential user always resolves even
    //    without (or with a non-matching) repo hint. Several entries and
    //    no match is NOT "no token": the user is signed in, just not
    //    for this directory — say so, token-free.
    let entry = store
        .resolve_for(checkout.ok())
        .ok_or_else(|| AuthError::NoEntryForCheckout {
            checkout: checkout.map(str::to_string).map_err(str::to_string),
            path: super::store::credentials_path_with(xdg_config_home, home_override)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            entries: store.entries.iter().map(|e| e.summary()).collect(),
        })?;
    Ok(ResolvedCreds {
        token: entry.token.clone(),
        server: entry.server.clone(),
        user_login: entry.user_login.clone(),
        user_id: entry.user_id,
        repo: entry.repo.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::store::save_with;
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct TestEnv {
        _tmp: TempDir,
        xdg: PathBuf,
        creds: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let xdg = tmp.path().join("xdg");
            let creds = xdg.join("aristo/credentials");
            Self {
                _tmp: tmp,
                xdg,
                creds,
            }
        }

        fn xdg_str(&self) -> &str {
            self.xdg.to_str().unwrap()
        }

        fn write_creds(&self, body: &str) {
            fs::create_dir_all(self.creds.parent().unwrap()).unwrap();
            fs::write(&self.creds, body).unwrap();
        }
    }

    fn dummy_home() -> Option<&'static Path> {
        Some(Path::new("/nonexistent-test-home"))
    }

    #[test]
    fn env_var_takes_precedence_over_file() {
        let env = TestEnv::new();
        env.write_creds(
            r#"
[aretta]
token = "file-token"
issued_at = "2026-05-20T00:00:00Z"
"#,
        );
        let tok = resolve_with(Some("env-token"), Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(tok.as_str(), "env-token");
    }

    #[test]
    fn falls_back_to_credentials_file() {
        let env = TestEnv::new();
        env.write_creds(
            r#"
[aretta]
token = "file-token"
issued_at = "2026-05-20T00:00:00Z"
"#,
        );
        let tok = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(tok.as_str(), "file-token");
    }

    #[test]
    fn no_token_when_nothing_configured() {
        let env = TestEnv::new();
        let err = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert_eq!(err, AuthError::NoToken);
    }

    #[test]
    fn empty_env_var_falls_through_to_file() {
        let env = TestEnv::new();
        env.write_creds(
            r#"
[aretta]
token = "file-token"
issued_at = "2026-05-20T00:00:00Z"
"#,
        );
        let tok = resolve_with(Some("   "), Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(tok.as_str(), "file-token");
    }

    #[test]
    fn malformed_credentials_surfaces_useful_error() {
        let env = TestEnv::new();
        env.write_creds("this is not TOML at all = = =");
        let err = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert!(matches!(err, AuthError::Malformed(_)));
    }

    #[test]
    fn empty_token_in_file_rejects_with_malformed() {
        let env = TestEnv::new();
        env.write_creds(
            r#"
[aretta]
token = ""
issued_at = "2026-05-20T00:00:00Z"
"#,
        );
        let err = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert!(matches!(err, AuthError::Malformed(_)));
    }

    #[test]
    fn save_then_resolve_round_trip() {
        let env = TestEnv::new();
        save_with(
            &Token::new("round-trip-tok"),
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
        let tok = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(tok.as_str(), "round-trip-tok");
    }

    #[test]
    fn xdg_config_home_used_by_resolve() {
        // Mirror of the save test — resolve should look in the XDG
        // path too when that override is supplied.
        let env = TestEnv::new();
        save_with(&Token::new("xdg-tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        let tok = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(tok.as_str(), "xdg-tok");
    }

    // ─── multi-repo resolution (repo hint) ───────────────────────────────────

    use crate::auth::store::{save_store_with, CredentialEntry, CredentialStore};

    fn v2_entry(repo: &str, token: &str, minted_at: &str) -> CredentialEntry {
        CredentialEntry {
            server: ServerUrl::Prod,
            repo: Some(repo.to_string()),
            token: Token::new(token),
            minted_at: minted_at.to_string(),
            user_login: None,
            user_id: None,
        }
    }

    fn write_v2(env: &TestEnv, entries: Vec<CredentialEntry>) {
        save_store_with(
            &CredentialStore { entries },
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
    }

    #[test]
    fn repo_hint_selects_the_matching_entry() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![
                v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z"),
                v2_entry("owner/b", "tok-b", "2026-07-22T01:00:00Z"),
            ],
        );
        let creds =
            resolve_full_with(None, Some(env.xdg_str()), dummy_home(), Some("owner/b")).unwrap();
        assert_eq!(creds.token.as_str(), "tok-b");
        assert_eq!(creds.repo.as_deref(), Some("owner/b"));
    }

    #[test]
    fn sole_entry_resolves_without_a_hint() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/only", "tok", "2026-07-22T00:00:00Z")],
        );
        let creds = resolve_full_with(None, Some(env.xdg_str()), dummy_home(), None).unwrap();
        assert_eq!(creds.token.as_str(), "tok");
    }

    #[test]
    fn sole_entry_grace_covers_a_mismatched_hint() {
        // One credential, but the cwd repo doesn't match it — the single
        // entry still resolves (backward-compatible single-repo grace).
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z")],
        );
        let creds = resolve_full_with(
            None,
            Some(env.xdg_str()),
            dummy_home(),
            Some("owner/elsewhere"),
        )
        .unwrap();
        assert_eq!(creds.token.as_str(), "tok-a");
    }

    #[test]
    fn multi_entry_no_match_reports_what_is_on_file_for_the_checkout() {
        // Several credentials, none matching the repo hint, no sole
        // fallback → not "no token" (the user IS signed in) but "none of
        // these is for this checkout", carrying the derived repo and the
        // token-free entries so the CLI can say exactly that.
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![
                v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z"),
                v2_entry("owner/b", "tok-b", "2026-07-22T01:00:00Z"),
            ],
        );
        let err = resolve_full_with(None, Some(env.xdg_str()), dummy_home(), Some("owner/c"))
            .unwrap_err();
        match err {
            AuthError::NoEntryForCheckout {
                checkout,
                path,
                entries,
            } => {
                assert_eq!(checkout.as_deref().map_err(String::as_str), Ok("owner/c"));
                assert_eq!(path, env.creds.display().to_string());
                let repos: Vec<_> = entries.iter().map(|e| e.repo.as_deref()).collect();
                assert_eq!(repos, vec![Some("owner/a"), Some("owner/b")]);
                let rendered = format!("{entries:?}");
                assert!(!rendered.contains("tok-a"), "token leaked: {rendered}");
            }
            other => panic!("expected NoEntryForCheckout, got {other:?}"),
        }
    }

    #[test]
    fn multi_entry_with_underivable_checkout_carries_the_derivation_error() {
        // The cwd is not a GitHub checkout: the resolver still cannot
        // pick among several entries, and the error says WHY no repo was
        // derived instead of silently reporting "no token".
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![
                v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z"),
                v2_entry("owner/b", "tok-b", "2026-07-22T01:00:00Z"),
            ],
        );
        let err = resolve_full_for_checkout(
            None,
            Some(env.xdg_str()),
            dummy_home(),
            Err("no .git/config at /tmp/x/.git/config"),
        )
        .unwrap_err();
        match err {
            AuthError::NoEntryForCheckout { checkout, .. } => {
                assert_eq!(
                    checkout.as_deref().map_err(String::as_str),
                    Err("no .git/config at /tmp/x/.git/config")
                );
            }
            other => panic!("expected NoEntryForCheckout, got {other:?}"),
        }
    }

    #[test]
    fn empty_store_is_still_no_token() {
        // Nothing on file at all → the plain NoToken (the free-tier
        // nudge stays correct for this case).
        let env = TestEnv::new();
        let err = resolve_full_with(None, Some(env.xdg_str()), dummy_home(), Some("owner/c"))
            .unwrap_err();
        assert_eq!(err, AuthError::NoToken);
    }

    #[test]
    fn env_token_bypasses_the_store() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![
                v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z"),
                v2_entry("owner/b", "tok-b", "2026-07-22T01:00:00Z"),
            ],
        );
        let creds = resolve_full_with(
            Some("env-tok"),
            Some(env.xdg_str()),
            dummy_home(),
            Some("owner/a"),
        )
        .unwrap();
        assert_eq!(creds.token.as_str(), "env-tok");
        assert_eq!(creds.server, ServerUrl::Prod);
    }
}
