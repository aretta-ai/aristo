//! Credentials-file atomic I/O. The on-disk store for the token.
//!
//! ## File location
//!
//! Honors `$XDG_CONFIG_HOME` when set (Linux convention); else falls
//! back to a per-OS default:
//!
//! - Linux:   `~/.config/aristo/credentials`
//! - macOS:   `~/Library/Application Support/aristo/credentials`
//!
//! On Unix, the file is created with mode `0600` (owner-only).
//!
//! ## Atomic writes
//!
//! `save_with` writes to `<path>.tmp` then `rename`s. Either the new
//! file exists in full, or the old one is untouched — no half-written
//! credentials.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::AuthError;
use super::token::Token;

/// Filename inside the per-user config directory.
pub const CREDENTIALS_FILENAME: &str = "credentials";

/// Persist a token to the credentials file. Reads `$XDG_CONFIG_HOME`
/// and `$HOME` from the process env to determine the destination
/// path — same precedence as [`super::resolve::resolve`] so that
/// `aristo auth login` and the next API call agree on which file to
/// touch.
pub fn save(token: &Token) -> io::Result<()> {
    save_with(
        token,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Persist with explicit home-dir override (no XDG override). Used by
/// tests that pin behavior to a particular `$HOME` without touching
/// `$XDG_CONFIG_HOME`.
pub fn save_with_home(token: &Token, home_override: Option<&Path>) -> io::Result<()> {
    save_with(token, None, home_override)
}

/// Persist with full env-var + home-dir overrides. The XDG override
/// controls only the path resolution; the actual token is the `token`
/// argument.
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

pub(super) fn credentials_path_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<PathBuf, AuthError> {
    let dir = config_dir_with(xdg_config_home, home_override)?;
    Ok(dir.join(CREDENTIALS_FILENAME))
}

/// Aristo's config directory: `$XDG_CONFIG_HOME/aristo` (or
/// `~/.config/aristo`) on Linux; `~/Library/Application Support/aristo`
/// on macOS.
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

pub(super) fn home_dir() -> Option<PathBuf> {
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
pub(super) struct CredentialsFile {
    pub(super) aretta: AretaCredentials,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AretaCredentials {
    pub(super) token: String,
    #[serde(default)]
    #[allow(dead_code)] // Persisted for audit + future expiry checks.
    pub(super) issued_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test environment: a TempDir + a pinned XDG path so tests are
    /// platform-independent (macOS would otherwise resolve to
    /// `~/Library/Application Support` instead of `~/.config`).
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
    }

    fn dummy_home() -> Option<&'static Path> {
        Some(Path::new("/nonexistent-test-home"))
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
    fn credentials_path_combines_xdg_with_filename() {
        let env = TestEnv::new();
        let p = credentials_path_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(p, env.creds);
    }
}
