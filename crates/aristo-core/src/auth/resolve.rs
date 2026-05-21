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

use std::fs;
use std::path::Path;

use super::error::AuthError;
use super::server::ServerUrl;
use super::store::{credentials_path_with, home_dir, CredentialsFile};
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
/// `unsafe_code`, which `std::env::set_var` requires).
pub fn resolve_with(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<Token, AuthError> {
    Ok(resolve_full_with(env_token, xdg_config_home, home_override)?.token)
}

/// Like [`resolve`] but returns the full credentials record (server,
/// user, repo) when available. Use this from canon command call
/// sites that need the server URL paired with the token.
pub fn resolve_full() -> Result<ResolvedCreds, AuthError> {
    resolve_full_with(
        std::env::var(ENV_VAR).ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Resolve a full credentials record with explicit overrides.
pub fn resolve_full_with(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
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
    // 2. On-disk credentials file.
    let path = credentials_path_with(xdg_config_home, home_override)?;
    if !path.exists() {
        return Err(AuthError::NoToken);
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| AuthError::Malformed(format!("read {}: {e}", path.display())))?;
    // Try TOML first. If it doesn't parse and the file looks like a
    // bare-token (old format pre-commit-4), accept that for back-compat.
    let parsed: CredentialsFile = match toml::from_str(&raw) {
        Ok(p) => p,
        Err(_) => {
            // Back-compat: maybe it's a bare token (no TOML structure).
            // The OLD format was the [aretta] TOML shape; a plain text
            // file with no `[aretta]` header would only come from a
            // pre-PR-1 ancestor or a hand-edited file. Treat as a bare
            // token if it's a single non-empty line.
            let token = raw.trim();
            if token.is_empty() || token.contains('=') || token.contains('[') {
                return Err(AuthError::Malformed(format!(
                    "credentials file at {} is not parseable",
                    path.display()
                )));
            }
            return Ok(ResolvedCreds {
                token: Token::new(token),
                server: ServerUrl::Prod,
                user_login: None,
                user_id: None,
                repo: None,
            });
        }
    };
    let token = parsed.aretta.token.trim();
    if token.is_empty() {
        return Err(AuthError::Malformed(format!(
            "credentials file at {} has an empty token",
            path.display()
        )));
    }
    let server = parsed
        .aretta
        .server
        .as_deref()
        .map(ServerUrl::parse)
        .unwrap_or(ServerUrl::Prod);
    Ok(ResolvedCreds {
        token: Token::new(token),
        server,
        user_login: parsed.aretta.user_login.clone(),
        user_id: parsed.aretta.user_id,
        repo: parsed.aretta.repo.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::store::save_with;
    use super::*;
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
}
