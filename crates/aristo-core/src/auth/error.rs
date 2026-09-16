//! Auth-error variants.
//!
//! Lifted from `canon::client` so non-canon features can use the same
//! error vocabulary. `canon::client` re-exports for backward compat.

use std::fmt;

/// The one way to sign in, ready to paste. Every hint that tells a user
/// to log in goes through here so they all say the same thing.
pub fn login_command() -> String {
    "aristo auth login --server https://<org>.aretta.ai".to_string()
}

/// Why an auth-token resolution failed. Cleaved so the SDK can
/// surface the right user hint (login vs token-expired vs
/// CI-token-missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No `ARETTA_TOKEN` env var, no `~/.config/aristo/credentials`.
    /// Its message is the complete sign-in hint; callers print it as is.
    NoToken,
    /// Token present but server rejected it (401). Likely expired
    /// or revoked.
    Invalid,
    /// Token present but the resolution code couldn't read /
    /// parse it (filesystem error, malformed credentials file).
    Malformed(String),
    /// Credentials for several servers are on file and nothing selected
    /// one: pass `--server` or set `ARETTA_API_URL`. Distinct from
    /// [`AuthError::NoToken`] (nothing on file): the user is signed in.
    SeveralServers {
        /// Where the credentials file lives, for the user's orientation.
        path: String,
        /// The stored entries, token-free.
        entries: Vec<EntrySummary>,
    },
    /// The org's server could not be reached (DNS, connect, timeout).
    Unreachable {
        /// The server (data-plane base) that was addressed.
        server: String,
        /// The transport's reason.
        reason: String,
    },
    /// The checkout's GitHub repo is not one of the org's repos at this
    /// server — the org's directory has no entry for it.
    RepoNotInOrg {
        /// The checkout's `owner/repo`.
        github_repo: String,
        /// The org's server.
        server: String,
    },
    /// `ARETTA_TOKEN` is set but `ARETTA_API_URL` is not. An env token
    /// carries no server, and the platform apex cannot serve an org's
    /// data plane, so the server must be named.
    EnvTokenWithoutServer,
}

/// A stored credential minus its secret — what the CLI may print when
/// listing what is on file. Deliberately carries no token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    /// The org's server the entry is keyed under (display form).
    pub server: String,
    /// GitHub login recorded at mint time, if any.
    pub user_login: Option<String>,
}

impl fmt::Display for EntrySummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "server: {}", self.server)?;
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
                "not signed in: run `{}` (the org host is your Aretta dashboard's hostname), \
                 or set ARETTA_TOKEN + ARETTA_API_URL",
                login_command()
            ),
            AuthError::Invalid => write!(f, "auth token rejected by server (expired or revoked)"),
            AuthError::Malformed(msg) => write!(f, "credentials malformed: {msg}"),
            AuthError::Unreachable { server, reason } => {
                write!(f, "could not reach {server}: {reason}")
            }
            AuthError::RepoNotInOrg {
                github_repo,
                server,
            } => write!(
                f,
                "{github_repo} is not a repo of {server} — run from a checkout of one of the org's \
                 repos, or set ARETTA_API_URL to the org that has it"
            ),
            AuthError::EnvTokenWithoutServer => write!(
                f,
                "ARETTA_TOKEN is set but ARETTA_API_URL is not — set \
                 ARETTA_API_URL=https://<org>.aretta.ai (the server the token was minted against)"
            ),
            AuthError::SeveralServers { path, entries } => {
                writeln!(
                    f,
                    "signed in to several servers; say which: pass `--server <url>` or set ARETTA_API_URL."
                )?;
                writeln!(f, "  on file ({path}):")?;
                for e in entries {
                    writeln!(f, "    • {e}")?;
                }
                write!(
                    f,
                    "  (`aristo auth logout --server <url>` drops one you no longer use.)"
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
        assert!(
            s.contains("aristo auth login --server https://<org>.aretta.ai"),
            "got: {s}"
        );
        assert!(
            s.contains("ARETTA_TOKEN") && s.contains("ARETTA_API_URL"),
            "got: {s}"
        );
        assert!(!s.contains("tier") && !s.contains("trial"), "got: {s}");
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
                user_login: Some("alice".into()),
            },
            EntrySummary {
                server: "https://other.aretta.ai".into(),
                user_login: None,
            },
        ]
    }

    #[test]
    fn display_several_servers_lists_servers_and_both_selectors() {
        let s = AuthError::SeveralServers {
            path: "/home/alice/.config/aristo/credentials".into(),
            entries: two_entries(),
        }
        .to_string();
        assert!(
            s.contains("--server <url>") && s.contains("ARETTA_API_URL"),
            "got: {s}"
        );
        assert!(
            s.contains("/home/alice/.config/aristo/credentials"),
            "got: {s}"
        );
        assert!(
            s.contains("server: https://acme.aretta.ai   user: alice"),
            "got: {s}"
        );
        assert!(s.contains("server: https://other.aretta.ai"), "got: {s}");
        assert!(!s.contains("tier") && !s.contains("trial"), "got: {s}");
    }

    #[test]
    fn display_env_token_without_server_names_both_variables() {
        let s = AuthError::EnvTokenWithoutServer.to_string();
        assert!(
            s.contains("ARETTA_TOKEN") && s.contains("ARETTA_API_URL"),
            "got: {s}"
        );
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
