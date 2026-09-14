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
    /// Credentials ARE on file, but none of them is for the current
    /// checkout: several entries stored, none scoped to the checkout's
    /// `owner/repo`, so the sole-entry grace does not apply either.
    /// Distinct from [`AuthError::NoToken`] (nothing on file) because
    /// the remedy is different — the user is signed in, just not for
    /// this directory — and the "start a trial" nudge would be wrong.
    NoEntryForCheckout {
        /// `Ok(owner/repo)` derived from the cwd's git remote, or
        /// `Err(why)` when it could not be derived (not a git checkout,
        /// no `origin`, non-GitHub URL).
        checkout: Result<String, String>,
        /// Where the credentials file lives, for the user's orientation.
        path: String,
        /// The stored entries, token-free.
        entries: Vec<EntrySummary>,
    },
}

/// A stored credential minus its secret — what the CLI may print when
/// listing what is on file. Deliberately carries no token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    /// Login URL the entry is keyed under (display form).
    pub server: String,
    /// `owner/repo` the entry is scoped to, if known.
    pub repo: Option<String>,
    /// GitHub login recorded at mint time, if any.
    pub user_login: Option<String>,
}

impl fmt::Display for EntrySummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "server: {}   repo: {}",
            self.server,
            self.repo.as_deref().unwrap_or("(unscoped)")
        )?;
        if let Some(user) = &self.user_login {
            write!(f, "   user: {user}")?;
        }
        Ok(())
    }
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
            AuthError::NoEntryForCheckout {
                checkout,
                path,
                entries,
            } => {
                writeln!(
                    f,
                    "signed in, but no stored credential is for this checkout."
                )?;
                match checkout {
                    Ok(repo) => writeln!(
                        f,
                        "  this checkout: {repo} (from .git/config remote.origin.url)"
                    )?,
                    Err(why) => {
                        writeln!(f, "  this checkout: could not derive owner/repo — {why}")?
                    }
                }
                writeln!(f, "  on file ({path}):")?;
                for e in entries {
                    writeln!(f, "    • {e}")?;
                }
                let login_repo = match checkout {
                    Ok(repo) => repo.as_str(),
                    Err(_) => "<owner/repo>",
                };
                writeln!(
                    f,
                    "  fix: run `aristo auth login --repo {login_repo}` from this checkout,"
                )?;
                write!(
                    f,
                    "       or set ARETTA_TOKEN (`aristo auth token --repo <owner/repo>` prints a stored one)."
                )
            }
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

    fn two_entries() -> Vec<EntrySummary> {
        vec![
            EntrySummary {
                server: "https://acme.aretta.ai".into(),
                repo: Some("acme/widgets".into()),
                user_login: Some("alice".into()),
            },
            EntrySummary {
                server: "https://code.aretta.ai".into(),
                repo: None,
                user_login: None,
            },
        ]
    }

    #[test]
    fn display_no_entry_for_checkout_with_derived_repo_lists_entries_and_remedies() {
        let s = AuthError::NoEntryForCheckout {
            checkout: Ok("alice/widgets".into()),
            path: "/home/alice/.config/aristo/credentials".into(),
            entries: two_entries(),
        }
        .to_string();
        // Never the trial nudge; the user IS signed in.
        assert!(!s.contains("trial"), "got: {s}");
        assert!(s.contains("this checkout: alice/widgets"), "got: {s}");
        assert!(
            s.contains("/home/alice/.config/aristo/credentials"),
            "got: {s}"
        );
        assert!(
            s.contains("server: https://acme.aretta.ai   repo: acme/widgets   user: alice"),
            "got: {s}"
        );
        assert!(
            s.contains("server: https://code.aretta.ai   repo: (unscoped)"),
            "got: {s}"
        );
        // Remedy 1 names the derived repo; remedy 2 is the env var.
        assert!(
            s.contains("`aristo auth login --repo alice/widgets` from this checkout"),
            "got: {s}"
        );
        assert!(s.contains("ARETTA_TOKEN"), "got: {s}");
    }

    #[test]
    fn display_no_entry_for_checkout_with_derivation_error_shows_the_reason() {
        let s = AuthError::NoEntryForCheckout {
            checkout: Err(
                "remote.origin.url `https://gitlab.com/x/y` doesn't look like a GitHub URL".into(),
            ),
            path: "/c".into(),
            entries: two_entries(),
        }
        .to_string();
        assert!(s.contains("could not derive owner/repo"), "got: {s}");
        assert!(s.contains("doesn't look like a GitHub URL"), "got: {s}");
        // No derived repo to plug in — the placeholder form of remedy 1.
        assert!(
            s.contains("--repo <owner/repo>` from this checkout"),
            "got: {s}"
        );
        assert!(s.contains("ARETTA_TOKEN"), "got: {s}");
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
