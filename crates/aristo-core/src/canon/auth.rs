//! Auth-token resolution + persistence for the canon API.
//!
//! Three sources, checked in order:
//!
//! 1. `ARETTA_TOKEN` env var — CI-friendly; takes precedence over
//!    the on-disk credentials file so `ARETTA_TOKEN=… cargo test`
//!    works without touching `~/.config/aristo/credentials`.
//! 2. `~/.config/aristo/credentials` TOML file — the persistent
//!    store, written by `aristo auth login` and removed by
//!    `aristo auth logout`.
//! 3. No token → [`AuthError::NoToken`]. The SDK surfaces "run
//!    `aristo auth login`" as the recovery hint.
//!
//! ## Credentials file shape
//!
//! ```toml
//! [aretta]
//! token = "..."
//! issued_at = "2026-05-20T16:00:00Z"
//! ```
//!
//! The file lives under [`config_dir()`] — typically
//! `~/.config/aristo/credentials` on Linux, `~/Library/Application
//! Support/aristo/credentials` on macOS. We honor `$XDG_CONFIG_HOME`
//! on Linux when set.
//!
//! ## File permissions
//!
//! On Unix, the credentials file is created with mode `0600`
//! (owner-only read/write). The directory containing it inherits
//! whatever umask is active.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::client::AuthError;

/// Environment variable that overrides the on-disk credentials.
pub const ENV_VAR: &str = "ARETTA_TOKEN";

/// Filename inside [`config_dir()`].
pub const CREDENTIALS_FILENAME: &str = "credentials";

/// Resolved auth token, ready to send as `Authorization: Bearer <token>`.
///
/// Wrapping in a newtype prevents the raw string from being logged
/// or formatted accidentally: [`Display`](std::fmt::Display) is
/// **not** implemented; callers reach the underlying string via
/// [`Token::as_str`].
#[derive(Clone)]
pub struct Token(String);

impl Token {
    /// Underlying token string. Safe to send as `Authorization:
    /// Bearer <token>`; do not log this value.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct a token directly. Used by tests + [`save`].
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the body so accidental `{:?}` logging doesn't leak.
        // Show length only for the "is this empty?" debugging case.
        write!(f, "Token(<redacted; {} chars>)", self.0.len())
    }
}

/// Resolve the auth token via the documented precedence:
/// env var → credentials file → [`AuthError::NoToken`].
///
/// Callers wrap this result; in normal operation the token is held
/// by an HTTP client across calls (no need to re-resolve per call).
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
///
/// - `env_token` is the value of `ARETTA_TOKEN` (or `None` if unset)
/// - `xdg_config_home` is the value of `XDG_CONFIG_HOME` (or `None`)
/// - `home_override` is the user's home directory
pub fn resolve_with(
    env_token: Option<&str>,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<Token, AuthError> {
    // 1. Env var first — CI-friendly precedence.
    if let Some(t) = env_token {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(Token::new(t));
        }
    }
    // 2. On-disk credentials file.
    let path = credentials_path_with(xdg_config_home, home_override)?;
    if !path.exists() {
        return Err(AuthError::NoToken);
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| AuthError::Malformed(format!("read {}: {e}", path.display())))?;
    let parsed: CredentialsFile = toml::from_str(&raw)
        .map_err(|e| AuthError::Malformed(format!("parse {}: {e}", path.display())))?;
    let token = parsed.aretta.token.trim();
    if token.is_empty() {
        return Err(AuthError::Malformed(format!(
            "credentials file at {} has an empty token",
            path.display()
        )));
    }
    Ok(Token::new(token))
}

/// Persist a token to the credentials file. Creates parent
/// directories as needed. On Unix, sets file mode to `0600`.
///
/// Overwrites any existing file (the typical case for re-login
/// after token rotation).
///
/// Reads `$XDG_CONFIG_HOME` and `$HOME` from the process env to
/// determine the destination path — same precedence as
/// [`resolve`] so that `aristo auth login` and the next canon
/// API call agree on which file to touch.
pub fn save(token: &Token) -> io::Result<()> {
    save_with(
        token,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Persist with explicit home-dir override (no XDG override). Used
/// by tests that want to pin behavior to a particular `$HOME`
/// without touching `$XDG_CONFIG_HOME`.
pub fn save_with_home(token: &Token, home_override: Option<&Path>) -> io::Result<()> {
    save_with(token, None, home_override)
}

/// Persist with full env-var + home-dir overrides. The env-var
/// override controls only the `XDG_CONFIG_HOME` resolution; the
/// actual token is the `token` arg.
pub fn save_with(
    token: &Token,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<()> {
    let path = credentials_path_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = CredentialsFile {
        aretta: AretaCredentials {
            token: token.as_str().to_string(),
            issued_at: now_iso8601(),
        },
    };
    let toml_text = toml::to_string_pretty(&body)
        .map_err(|e| io::Error::other(format!("serialize credentials: {e}")))?;

    // Write atomically: write to <path>.tmp then rename.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, toml_text.as_bytes())?;
    #[cfg(unix)]
    set_unix_owner_only(&tmp)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove the credentials file, if it exists. Idempotent — missing
/// file is not an error.
///
/// Reads `$XDG_CONFIG_HOME` and `$HOME` from the process env to
/// determine which file to remove — same precedence as [`save`]
/// and [`resolve`].
pub fn clear() -> io::Result<()> {
    clear_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Clear with an explicit home-dir override (no XDG override).
pub fn clear_with_home(home_override: Option<&Path>) -> io::Result<()> {
    clear_with(None, home_override)
}

/// Clear with explicit env-var + home-dir overrides.
pub fn clear_with(xdg_config_home: Option<&str>, home_override: Option<&Path>) -> io::Result<()> {
    let path = credentials_path_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Absolute path to the credentials file (does not check existence).
pub fn credentials_path() -> Result<PathBuf, AuthError> {
    credentials_path_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

fn credentials_path_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<PathBuf, AuthError> {
    // For resolve(), the production code path needs the env-var
    // visible; injecting None here would lose that. So we look at
    // the env when xdg_config_home is None — this matches "test
    // explicitly passes None to mean 'no XDG override'".
    let dir = config_dir_with(xdg_config_home, home_override)?;
    Ok(dir.join(CREDENTIALS_FILENAME))
}

/// Aristo's config directory: `$XDG_CONFIG_HOME/aristo` (or
/// `~/.config/aristo`) on Linux; `~/Library/Application Support/aristo`
/// on macOS. Falls back to `~/.aristo` if neither is available.
pub fn config_dir() -> Result<PathBuf, AuthError> {
    config_dir_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

fn config_dir_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<PathBuf, AuthError> {
    // If caller explicitly passes Some(xdg), use it.
    if let Some(xdg) = xdg_config_home {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            return Ok(PathBuf::from(xdg).join("aristo"));
        }
    }
    let home = home_override.ok_or_else(|| {
        AuthError::Malformed("could not determine $HOME for credentials file".into())
    })?;
    if cfg!(target_os = "macos") {
        Ok(home.join("Library/Application Support/aristo"))
    } else {
        Ok(home.join(".config/aristo"))
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn io_from_auth_error(e: AuthError) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(unix)]
fn set_unix_owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
}

fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

// ─── on-disk schema ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CredentialsFile {
    aretta: AretaCredentials,
}

#[derive(Debug, Serialize, Deserialize)]
struct AretaCredentials {
    token: String,
    #[serde(default)]
    #[allow(dead_code)] // Persisted for audit + future expiry checks.
    issued_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Tests use the `_with` variants exclusively so they can run in
    // parallel without touching process-wide env vars (the workspace
    // forbids `unsafe_code` which `std::env::set_var` requires).
    //
    // Tests always pass an explicit XDG override so the path is
    // platform-independent — without it macOS would use
    // `~/Library/Application Support/aristo` and Linux would use
    // `~/.config/aristo`. This setup forces the same layout
    // (`<xdg>/aristo/credentials`) everywhere.

    /// Builds a (TempDir, xdg-path, creds-path) tuple. Always pass
    /// `Some(xdg_str)` to the auth fns to pin behavior.
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

    // A dummy home_override is required by the API but ignored when
    // xdg_config_home is Some.
    fn dummy_home() -> Option<&'static Path> {
        Some(Path::new("/nonexistent-test-home"))
    }

    #[test]
    fn resolve_env_var_takes_precedence_over_file() {
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
    fn resolve_falls_back_to_credentials_file() {
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
    fn resolve_no_token_when_nothing_configured() {
        let env = TestEnv::new();
        let err = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert_eq!(err, AuthError::NoToken);
    }

    #[test]
    fn resolve_empty_env_var_falls_through_to_file() {
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
    fn resolve_malformed_credentials_surfaces_useful_error() {
        let env = TestEnv::new();
        env.write_creds("this is not TOML at all = = =");
        let err = resolve_with(None, Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert!(matches!(err, AuthError::Malformed(_)));
    }

    #[test]
    fn resolve_empty_token_in_file_rejects_with_malformed() {
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
    fn save_creates_parent_directory() {
        let env = TestEnv::new();
        assert!(!env.xdg.join("aristo").exists());
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(env.creds.exists());
    }

    #[test]
    #[cfg(unix)]
    fn save_sets_owner_only_unix_perms() {
        use std::os::unix::fs::PermissionsExt;
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        let meta = fs::metadata(&env.creds).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "credentials file should be owner-only readable, got {mode:o}"
        );
    }

    #[test]
    fn clear_removes_file() {
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(env.creds.exists());
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(!env.creds.exists());
    }

    #[test]
    fn clear_when_file_missing_is_not_an_error() {
        let env = TestEnv::new();
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
        // Idempotent — calling twice is fine.
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
    }

    #[test]
    fn xdg_config_home_lands_file_under_xdg_path() {
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        // File lands under XDG path, not under the (fake) home.
        assert!(env.xdg.join("aristo/credentials").exists());
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

    #[test]
    fn token_debug_format_redacts_body() {
        let t = Token::new("my-secret-token-value");
        let s = format!("{t:?}");
        assert!(!s.contains("my-secret-token-value"), "got: {s}");
        assert!(s.contains("redacted"), "got: {s}");
    }
}
