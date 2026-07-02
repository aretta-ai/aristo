//! [`CanonClient`] trait + error types.
//!
//! Three production impls:
//!
//! - [`HttpCanonClient`](super::http_client) — real HTTP via
//!   `reqwest`. Lands in PR #3.
//! - [`NoopCanonClient`](super::noop_client) — free-tier path; every
//!   method returns [`CanonError::NotEnabled`] so the caller can
//!   surface a tier-appropriate upgrade nudge without branching on
//!   `Option<dyn CanonClient>`.
//! - [`MockCanonClient`](super::mock_client) — TOML-fixture-driven
//!   for tests. Reads canned responses from a directory pointed to
//!   by `ARISTO_CANON_FIXTURE`.
//!
//! The trait deliberately uses `&self` (shared reference) so
//! consumers can hold a `Box<dyn CanonClient>` across calls without
//! interior mutability dances. State (auth tokens, connection
//! reuse) lives inside the impl behind whatever sync primitive it
//! prefers (Arc/Mutex for HTTP, immutable for Noop, RefCell/Mutex
//! for Mock).

use std::fmt;

use super::types::{
    CanonCatalogue, CanonEntry, CanonMatchRequest, CanonMatchResponse, RequestVerifyBody,
    RequestVerifyResponse,
};

/// Trait abstracting over the canon API endpoints (`POST /canon/match`,
/// `GET /canon/entry/<id>`, `POST /canon/request-verify`).
///
/// All three methods return [`CanonError`] on failure. Callers
/// typically downgrade these to "skip canon for this run; warn once"
/// rather than aborting (the daily authoring loop continues even
/// when the server is unreachable).
pub trait CanonClient: Send + Sync {
    /// Batched match for one or more annotations. The response's
    /// `results` list is aligned to `req.annotations` by index.
    fn match_annotations(&self, req: &CanonMatchRequest) -> Result<CanonMatchResponse, CanonError>;

    /// Fetch full per-entry detail by canon id. Paid-tier auth-gated
    /// with per-token rate-limit (anti-enumeration); the response
    /// deliberately excludes closed-IP fields (`match_signals`,
    /// `verification_artifacts`, internal spec IDs) so the lookup
    /// surface is the user-visible trust-card content only. No
    /// match-history gate — the source-level `kanon:`/`aristos:`
    /// prefix is the binding evidence and is already public in the
    /// user's source. `version` is optional; when `None`, the server
    /// returns the entry's currently-active version per its
    /// `INDEX.yaml`.
    fn get_entry(&self, canon_id: &str, version: Option<&str>) -> Result<CanonEntry, CanonError>;

    /// Record the user's demand-signal for a canon entry's backing.
    /// Idempotent on `(canon_id, repo_full_name, user_id)` — repeat
    /// calls refresh `requested_at` and may update notes per
    /// canon-strategy.md §CS11.
    fn request_verify(&self, body: &RequestVerifyBody)
        -> Result<RequestVerifyResponse, CanonError>;

    /// Fetch the full active canon catalogue (`GET /catalogue`). One
    /// entry per canon id at its active version; closed-IP fields are
    /// stripped server-side. Requires auth (Read capability) and is
    /// served by a per-repo conductor, addressed via the resolved
    /// data-plane base.
    fn catalogue(&self) -> Result<CanonCatalogue, CanonError>;
}

/// Errors surfaced by [`CanonClient`] methods. Cleaved by recovery
/// pattern: each variant tells the caller whether to retry, warn,
/// or fail hard.
#[derive(Debug)]
pub enum CanonError {
    /// `[canon] enabled = false` in `aristo.toml`. Caller should
    /// skip silently — the user opted out.
    NotEnabled,
    /// No auth token resolved (env var unset, no credentials file).
    /// Caller should surface the "run `aristo auth login`" hint and
    /// downgrade canon to a warning, not an error (the daily loop
    /// continues).
    Auth(AuthError),
    /// Network failure before any HTTP response (DNS, connect,
    /// reset). Caller should warn once and skip canon for this run
    /// — cached matches remain valid.
    Network(String),
    /// 8-second timeout per L3's graceful-degradation policy.
    /// Same treatment as [`CanonError::Network`].
    Timeout,
    /// Server returned 4xx (other than 401). Common cases:
    ///
    /// - 400 — confidence threshold below the server-enforced
    ///   `0.5` floor (the SDK should already prevent this; bug if
    ///   it surfaces in prod).
    /// - 404 — `/canon/entry/<id>` for an entry that hasn't
    ///   appeared in the repo's match history.
    BadRequest { status: u16, message: String },
    /// Server returned 5xx. Same recovery treatment as
    /// [`CanonError::Network`].
    Server { status: u16, message: String },
    /// Response body didn't deserialize. Indicates a contract
    /// mismatch between SDK and server.
    Decode(String),
    /// Mock-client-specific: fixture file missing / unparseable.
    /// Only surfaces during tests; prod runs never see this.
    Fixture(String),
}

impl fmt::Display for CanonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonError::NotEnabled => write!(
                f,
                "canon disabled via aristo.toml `[canon] enabled = false`"
            ),
            CanonError::Auth(e) => write!(f, "canon auth error: {e}"),
            CanonError::Network(msg) => write!(f, "canon network error: {msg}"),
            CanonError::Timeout => write!(f, "canon request timed out (>8s)"),
            CanonError::BadRequest { status, message } => {
                write!(f, "canon server returned HTTP {status}: {message}")
            }
            CanonError::Server { status, message } => {
                write!(f, "canon server error (HTTP {status}): {message}")
            }
            CanonError::Decode(msg) => write!(f, "canon response decode error: {msg}"),
            CanonError::Fixture(msg) => write!(f, "canon fixture error: {msg}"),
        }
    }
}

impl std::error::Error for CanonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CanonError::Auth(e) => Some(e),
            _ => None,
        }
    }
}

// `AuthError` moved to `aristo_core::auth::error` as part of the
// auth-extraction refactor. Re-exported here for backward compat;
// new code should import from `crate::auth::AuthError` directly.
pub use crate::auth::AuthError;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn canon_error_displays_render_useful_messages() {
        let e = CanonError::NotEnabled;
        assert!(e.to_string().contains("aristo.toml"));
        assert!(e.to_string().contains("enabled"));

        let e = CanonError::Auth(AuthError::NoToken);
        assert!(e.to_string().contains("aristo auth login"));

        let e = CanonError::Timeout;
        assert!(e.to_string().contains("timed out"));

        let e = CanonError::BadRequest {
            status: 400,
            message: "threshold too low".into(),
        };
        assert!(e.to_string().contains("400"));
        assert!(e.to_string().contains("threshold too low"));
    }

    #[test]
    fn auth_error_chains_through_canon_error_source() {
        let e = CanonError::Auth(AuthError::NoToken);
        let src = e.source().expect("auth error should be sourced");
        assert!(src.to_string().contains("aristo auth login"));
    }
}
