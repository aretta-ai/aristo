//! Auth-token resolution — env pair → the store → an error.
//!
//! 1. `ARETTA_TOKEN` + `ARETTA_API_URL` — CI-friendly; takes precedence
//!    over the on-disk credentials file. The token carries no server,
//!    so an env token without `ARETTA_API_URL` is
//!    [`AuthError::EnvTokenWithoutServer`], never a guess.
//! 2. The stored entry for the server named by `ARETTA_API_URL`, else
//!    the single stored entry when exactly one is on file. An `arta_*`
//!    token is an org grant, valid from any directory, so with one
//!    server on file there is nothing to choose.
//!
//! Nothing on file → [`AuthError::NoToken`] (its message is the sign-in
//! hint); several servers and none named →
//! [`AuthError::SeveralServers`] (lists them, token-free).

use std::path::Path;

use super::error::AuthError;
use super::server::ServerUrl;
use super::store::{home_dir, load_store_with};
use super::token::Token;

/// Full resolved-credentials record: the token plus the server it was
/// minted against and, for a stored entry, the user. Returned by
/// [`resolve_full`].
#[derive(Debug, Clone)]
pub struct ResolvedCreds {
    pub token: Token,
    /// Aretta server this token was minted against — the org's host.
    pub server: ServerUrl,
    /// `Some(login)` when sourced from a credentials file with a
    /// recorded user. `None` for env-var source.
    pub user_login: Option<String>,
    /// Numeric GitHub user id at mint time.
    pub user_id: Option<u64>,
}

/// Environment variable that overrides the on-disk credentials.
pub const ENV_VAR: &str = "ARETTA_TOKEN";

/// Environment variable naming the server an [`ENV_VAR`] token was
/// minted against; also selects among stored entries and overrides the
/// data plane.
pub const SERVER_ENV_VAR: &str = "ARETTA_API_URL";

/// Resolve the credentials for this process: the env pair, else the
/// stored entry per the documented precedence. Callers typically wrap
/// the token in an HTTP client across calls; no need to re-resolve.
pub fn resolve_full() -> Result<ResolvedCreds, AuthError> {
    resolve_full_with(
        std::env::var(ENV_VAR).ok().as_deref(),
        std::env::var(SERVER_ENV_VAR).ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Resolve with explicit overrides. Tests use this to avoid mutating
/// process state (the workspace forbids `unsafe_code`, which
/// `std::env::set_var` requires).
pub fn resolve_full_with(
    env_token: Option<&str>,
    env_server: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<ResolvedCreds, AuthError> {
    let env_server = env_server
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ServerUrl::parse);
    // 1. Env pair first — CI-friendly precedence. The token carries no
    //    metadata, so its server must be named too.
    if let Some(t) = env_token.map(str::trim).filter(|t| !t.is_empty()) {
        let server = env_server.ok_or(AuthError::EnvTokenWithoutServer)?;
        return Ok(ResolvedCreds {
            token: Token::new(t),
            server,
            user_login: None,
            user_id: None,
        });
    }
    // 2. The store: the named server's entry, else the single entry.
    let store = load_store_with(xdg_config_home, home_override)?;
    if store.is_empty() {
        return Err(AuthError::NoToken);
    }
    let entry =
        store
            .resolve_for(env_server.as_ref())
            .ok_or_else(|| AuthError::SeveralServers {
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

    const ACME: &str = "https://acme.aretta.ai";
    const OTHER: &str = "https://other.aretta.ai";

    fn entry(server: &str, token: &str) -> CredentialEntry {
        CredentialEntry {
            server: ServerUrl::parse(server),
            token: Token::new(token),
            minted_at: "2026-09-15T00:00:00Z".to_string(),
            user_login: None,
            user_id: None,
        }
    }

    fn write(env: &TestEnv, entries: Vec<CredentialEntry>) {
        save_store_with(
            &CredentialStore { entries },
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
    }

    fn resolve(
        env: &TestEnv,
        token: Option<&str>,
        server: Option<&str>,
    ) -> Result<ResolvedCreds, AuthError> {
        resolve_full_with(token, server, Some(env.xdg_str()), dummy_home())
    }

    #[test]
    fn env_pair_takes_precedence_over_file() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "file-token")]);
        let creds = resolve(&env, Some("env-token"), Some(ACME)).unwrap();
        assert_eq!(creds.token.as_str(), "env-token");
        assert_eq!(creds.server, ServerUrl::Custom(ACME.into()));
        assert_eq!(creds.user_login, None);
    }

    #[test]
    fn env_server_is_normalized_like_every_server_spec() {
        let env = TestEnv::new();
        let creds = resolve(&env, Some("env-tok"), Some("acme.aretta.ai/")).unwrap();
        assert_eq!(creds.server, ServerUrl::Custom(ACME.into()));
    }

    #[test]
    fn env_token_without_a_server_is_an_error_not_a_guess() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "file-token")]);
        for server in [None, Some(""), Some("   ")] {
            let err = resolve(&env, Some("env-token"), server).unwrap_err();
            assert_eq!(err, AuthError::EnvTokenWithoutServer, "server={server:?}");
        }
    }

    #[test]
    fn blank_env_token_falls_through_to_the_file() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "file-token")]);
        assert_eq!(
            resolve(&env, Some("   "), None).unwrap().token.as_str(),
            "file-token"
        );
    }

    #[test]
    fn nothing_on_file_is_no_token() {
        let env = TestEnv::new();
        assert_eq!(resolve(&env, None, None).unwrap_err(), AuthError::NoToken);
    }

    #[test]
    fn the_single_entry_resolves_from_anywhere() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "tok-a")]);
        let creds = resolve(&env, None, None).unwrap();
        assert_eq!(creds.token.as_str(), "tok-a");
        assert_eq!(creds.server, ServerUrl::Custom(ACME.into()));
    }

    #[test]
    fn a_named_server_selects_among_several() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "tok-a"), entry(OTHER, "tok-b")]);
        assert_eq!(
            resolve(&env, None, Some(OTHER)).unwrap().token.as_str(),
            "tok-b"
        );
        assert_eq!(
            resolve(&env, None, Some(ACME)).unwrap().token.as_str(),
            "tok-a"
        );
    }

    #[test]
    fn several_servers_and_none_named_lists_them_token_free() {
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "tok-a"), entry(OTHER, "tok-b")]);
        match resolve(&env, None, None).unwrap_err() {
            AuthError::SeveralServers { path, entries } => {
                assert_eq!(path, env.creds.display().to_string());
                let servers: Vec<_> = entries.iter().map(|e| e.server.as_str()).collect();
                assert_eq!(servers, vec![ACME, OTHER]);
                assert!(!format!("{entries:?}").contains("tok-"), "token leaked");
            }
            other => panic!("expected SeveralServers, got {other:?}"),
        }
    }

    #[test]
    fn a_named_server_with_no_entry_is_several_servers_not_no_token() {
        // Signed in elsewhere: say what is on file rather than "not signed in".
        let env = TestEnv::new();
        write(&env, vec![entry(ACME, "tok-a")]);
        assert!(matches!(
            resolve(&env, None, Some(OTHER)).unwrap_err(),
            AuthError::SeveralServers { .. }
        ));
    }

    #[test]
    fn malformed_credentials_surfaces_useful_error() {
        let env = TestEnv::new();
        env.write_creds("this is not TOML at all = = =");
        assert!(matches!(
            resolve(&env, None, None).unwrap_err(),
            AuthError::Malformed(_)
        ));
    }
}
