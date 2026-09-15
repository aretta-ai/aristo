//! Auth-token resolution — env var → the checkout's entry → an error.
//!
//! Two sources, checked in order:
//!
//! 1. `ARETTA_TOKEN` env var — CI-friendly; takes precedence over
//!    the on-disk credentials file so `ARETTA_TOKEN=… cargo test`
//!    works without touching `~/.config/aristo/credentials`. It must
//!    come with `ARETTA_API_URL`, the server the token was minted
//!    against: an env token without a server is
//!    [`AuthError::EnvTokenWithoutServer`], never a guess.
//! 2. The entry in the per-user credentials file (under
//!    [`super::store::config_dir`]) scoped to the current checkout's
//!    `owner/repo`. No other entry applies — a credential is used
//!    exactly where its repo is checked out.
//!
//! Nothing on file → [`AuthError::NoToken`] ("run `aristo auth login`");
//! entries on file but none for this checkout →
//! [`AuthError::NoEntryForCheckout`] (says which, and how to fix).

use std::path::Path;

use super::error::AuthError;
use super::server::ServerUrl;
use super::store::{home_dir, load_store_with};
use super::token::Token;

/// Full resolved-credentials record: the token plus the server it was
/// minted against and, for a stored entry, the user and repo. Returned
/// by [`resolve_full`].
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

/// Environment variable naming the server an [`ENV_VAR`] token was
/// minted against; also the data-plane override for stored credentials.
pub const SERVER_ENV_VAR: &str = "ARETTA_API_URL";

/// Resolve the full credentials record (token, server, user, repo) for
/// the current directory: `ARETTA_TOKEN`, else the stored entry scoped
/// to the cwd's `owner/repo` (derived from `.git/config`). Callers
/// typically wrap the token in an HTTP client across calls; no need to
/// re-resolve per call.
pub fn resolve_full() -> Result<ResolvedCreds, AuthError> {
    let checkout = cwd_checkout();
    resolve_full_for_checkout(
        std::env::var(ENV_VAR).ok().as_deref(),
        std::env::var(SERVER_ENV_VAR).ok().as_deref(),
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
/// `repo_hint`. `None` is "no checkout given" — see
/// [`resolve_full_for_checkout`] to carry the reason a repo could not
/// be derived. Tests use the explicit overrides to avoid mutating
/// process state (the workspace forbids `unsafe_code`, which
/// `std::env::set_var` requires).
pub fn resolve_full_with(
    env_token: Option<&str>,
    env_server: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
    repo_hint: Option<&str>,
) -> Result<ResolvedCreds, AuthError> {
    resolve_full_for_checkout(
        env_token,
        env_server,
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
    env_server: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
    checkout: Result<&str, &str>,
) -> Result<ResolvedCreds, AuthError> {
    // 1. Env var first — CI-friendly precedence. The token carries no
    //    metadata, so its server must be named too (ARETTA_API_URL);
    //    no user/repo.
    if let Some(t) = env_token.map(str::trim).filter(|t| !t.is_empty()) {
        let server = env_server
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ServerUrl::parse)
            .ok_or(AuthError::EnvTokenWithoutServer)?;
        return Ok(ResolvedCreds {
            token: Token::new(t),
            server,
            user_login: None,
            user_id: None,
            repo: None,
        });
    }
    // 2. On-disk store (reads v2, migrates v1 + bare-token transparently).
    let store = load_store_with(xdg_config_home, home_override)?;
    if store.is_empty() {
        return Err(AuthError::NoToken);
    }
    // 3. The entry scoped to the current repo, and nothing else. Entries
    //    on file but no match is NOT "no token": the user is signed in,
    //    just not for this directory — say so, token-free.
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
    use super::*;
    use crate::auth::store::{save_store_with, CredentialEntry, CredentialStore};
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

    /// Resolve for `repo` with no env token.
    fn resolve_for(env: &TestEnv, repo: &str) -> Result<ResolvedCreds, AuthError> {
        resolve_full_with(None, None, Some(env.xdg_str()), dummy_home(), Some(repo))
    }

    const ORG: &str = "https://acme.aretta.ai";

    // ─── precedence ──────────────────────────────────────────────────────────

    #[test]
    fn env_var_takes_precedence_over_file() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "file-token", "2026-07-22T00:00:00Z")],
        );
        let creds = resolve_full_with(
            Some("env-token"),
            Some(ORG),
            Some(env.xdg_str()),
            dummy_home(),
            Some("owner/a"),
        )
        .unwrap();
        assert_eq!(creds.token.as_str(), "env-token");
        assert_eq!(creds.server, ServerUrl::Custom(ORG.into()));
        assert_eq!(creds.repo, None);
    }

    #[test]
    fn env_token_without_a_server_is_an_error_not_a_guess() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "file-token", "2026-07-22T00:00:00Z")],
        );
        for server in [None, Some(""), Some("   ")] {
            let err = resolve_full_with(
                Some("env-token"),
                server,
                Some(env.xdg_str()),
                dummy_home(),
                Some("owner/a"),
            )
            .unwrap_err();
            assert_eq!(err, AuthError::EnvTokenWithoutServer, "server={server:?}");
        }
    }

    #[test]
    fn empty_env_var_falls_through_to_file() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "file-token", "2026-07-22T00:00:00Z")],
        );
        let creds = resolve_full_with(
            Some("   "),
            None,
            Some(env.xdg_str()),
            dummy_home(),
            Some("owner/a"),
        )
        .unwrap();
        assert_eq!(creds.token.as_str(), "file-token");
    }

    #[test]
    fn no_token_when_nothing_configured() {
        let env = TestEnv::new();
        let err = resolve_for(&env, "owner/a").unwrap_err();
        assert_eq!(err, AuthError::NoToken);
    }

    // ─── file formats ────────────────────────────────────────────────────────

    #[test]
    fn v1_file_with_a_repo_resolves_for_that_repo() {
        let env = TestEnv::new();
        env.write_creds(
            r#"
[aretta]
token = "file-token"
issued_at = "2026-05-20T00:00:00Z"
repo = "owner/legacy"
"#,
        );
        let creds = resolve_for(&env, "owner/legacy").unwrap();
        assert_eq!(creds.token.as_str(), "file-token");
        assert_eq!(creds.repo.as_deref(), Some("owner/legacy"));
    }

    #[test]
    fn malformed_credentials_surfaces_useful_error() {
        let env = TestEnv::new();
        env.write_creds("this is not TOML at all = = =");
        let err = resolve_for(&env, "owner/a").unwrap_err();
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
        let err = resolve_for(&env, "owner/a").unwrap_err();
        assert!(matches!(err, AuthError::Malformed(_)));
    }

    // ─── selection: the checkout's repo, and nothing else ────────────────────

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
        let creds = resolve_for(&env, "owner/b").unwrap();
        assert_eq!(creds.token.as_str(), "tok-b");
        assert_eq!(creds.repo.as_deref(), Some("owner/b"));
    }

    #[test]
    fn a_single_entry_does_not_apply_to_another_checkout() {
        // No single-entry fallback: one credential on file, a checkout of
        // a different repo → not for this checkout, with the diagnosis.
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z")],
        );
        let err = resolve_for(&env, "owner/elsewhere").unwrap_err();
        match err {
            AuthError::NoEntryForCheckout {
                checkout, entries, ..
            } => {
                assert_eq!(
                    checkout.as_deref().map_err(String::as_str),
                    Ok("owner/elsewhere")
                );
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].repo.as_deref(), Some("owner/a"));
            }
            other => panic!("expected NoEntryForCheckout, got {other:?}"),
        }
    }

    #[test]
    fn an_unscoped_entry_never_resolves() {
        // A legacy entry with no repo cannot match any checkout.
        let env = TestEnv::new();
        env.write_creds("[aretta]\ntoken = \"bare\"\nissued_at = \"2026-05-20T00:00:00Z\"\n");
        let err = resolve_for(&env, "owner/a").unwrap_err();
        assert!(
            matches!(err, AuthError::NoEntryForCheckout { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn no_checkout_hint_resolves_nothing_from_the_file() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z")],
        );
        let err =
            resolve_full_with(None, None, Some(env.xdg_str()), dummy_home(), None).unwrap_err();
        match err {
            AuthError::NoEntryForCheckout { checkout, .. } => {
                assert_eq!(
                    checkout.as_deref().map_err(String::as_str),
                    Err("no checkout given")
                );
            }
            other => panic!("expected NoEntryForCheckout, got {other:?}"),
        }
    }

    #[test]
    fn multi_entry_no_match_reports_what_is_on_file_for_the_checkout() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![
                v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z"),
                v2_entry("owner/b", "tok-b", "2026-07-22T01:00:00Z"),
            ],
        );
        let err = resolve_for(&env, "owner/c").unwrap_err();
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
    fn underivable_checkout_carries_the_derivation_error() {
        let env = TestEnv::new();
        write_v2(
            &env,
            vec![v2_entry("owner/a", "tok-a", "2026-07-22T00:00:00Z")],
        );
        let err = resolve_full_for_checkout(
            None,
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
            Some("acme.aretta.ai/"),
            Some(env.xdg_str()),
            dummy_home(),
            Some("owner/a"),
        )
        .unwrap();
        assert_eq!(creds.token.as_str(), "env-tok");
        // The env server is normalized like every other server spec.
        assert_eq!(creds.server, ServerUrl::Custom(ORG.into()));
    }
}
