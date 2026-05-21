//! Auth-error variants.
//!
//! Lifted from `canon::client` so non-canon features can use the same
//! error vocabulary. `canon::client` re-exports for backward compat.

use std::fmt;

/// Why an auth-token resolution failed. Cleaved so the SDK can
/// surface the right user hint (login vs token-expired vs
/// CI-token-missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No `ARETTA_TOKEN` env var, no `~/.config/aristo/credentials`.
    /// Hint: run `aristo auth login` (or set `ARETTA_TOKEN` for CI).
    NoToken,
    /// Token present but server rejected it (401). Likely expired
    /// or revoked.
    Invalid,
    /// Token present but the resolution code couldn't read /
    /// parse it (filesystem error, malformed credentials file).
    Malformed(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::NoToken => write!(
                f,
                "no auth token configured \
                 (run `aristo auth login` or set ARETTA_TOKEN)"
            ),
            AuthError::Invalid => write!(f, "auth token rejected by server (expired or revoked)"),
            AuthError::Malformed(msg) => write!(f, "credentials malformed: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn display_no_token_mentions_login_command() {
        let s = AuthError::NoToken.to_string();
        assert!(s.contains("aristo auth login"), "got: {s}");
        assert!(s.contains("ARETTA_TOKEN"), "got: {s}");
    }

    #[test]
    fn display_invalid_mentions_revoked_or_expired() {
        let s = AuthError::Invalid.to_string();
        assert!(s.contains("expired") || s.contains("revoked"), "got: {s}");
    }

    #[test]
    fn display_malformed_includes_underlying_reason() {
        let s = AuthError::Malformed("parse error at line 3".into()).to_string();
        assert!(s.contains("parse error at line 3"), "got: {s}");
    }

    #[test]
    fn is_std_error() {
        // AuthError must implement std::error::Error so it composes
        // into other error chains.
        fn _assert_is_error<T: Error>(_: &T) {}
        let e = AuthError::NoToken;
        _assert_is_error(&e);
    }
}
