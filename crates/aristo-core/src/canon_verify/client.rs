//! [`VerifyClient`] trait + [`VerifyError`].
//!
//! Two production impls:
//!
//! - [`HttpVerifyClient`](super::http_client::HttpVerifyClient) — real
//!   HTTP via `ureq`. Bearer-token auth using the same `arta_*` resolved
//!   from [`crate::auth::resolve_full`].
//! - [`MockVerifyClient`](super::mock_client::MockVerifyClient) —
//!   in-process fake for tests; records the last request and returns
//!   a pre-canned response.
//!
//! Errors are cleaved by recovery pattern (mirror of [`crate::canon::CanonError`]).
//! Most variants surface to the SDK as a fail-loud message + exit; the
//! verify path doesn't graceful-degrade because the user explicitly asked
//! us to run remote verification.

use std::fmt;

use super::types::{GetVerifySessionResponse, PostVerifySessionResponse, VerifySessionRequest};

/// Trait abstracting the canon-verify endpoints.
///
/// Single round-trip per session: SDK POSTs once to create + dispatch,
/// then optionally polls GET for status. Implementations share a
/// bearer token + base URL state internally; consumers hold a
/// `Box<dyn VerifyClient>` across calls.
pub trait VerifyClient: Send + Sync {
    /// `POST /canon/verify/sessions` (WIRE 1). Returns 202 on success
    /// with `session_id` + `view_url`; the actual verification run
    /// proceeds asynchronously server-side.
    fn post_session(
        &self,
        req: &VerifySessionRequest,
    ) -> Result<PostVerifySessionResponse, VerifyError>;

    /// `GET /canon/verify/sessions/:id` (long-poll style; server may
    /// hold up to ~30 s server-side when implemented, but the SDK
    /// MUST tolerate immediate responses for the prototype where the
    /// `?wait=` hint is deferred).
    fn get_session(
        &self,
        session_id: &str,
        wait_seconds: Option<u32>,
    ) -> Result<GetVerifySessionResponse, VerifyError>;
}

/// Errors surfaced by [`VerifyClient`] methods.
///
/// Cleaved by recovery pattern; the SDK maps each variant to a
/// specific user-facing message + exit code in the CLI dispatcher.
#[derive(Debug)]
pub enum VerifyError {
    /// No auth resolved — `aristo verify` against canon-bound entries
    /// requires authentication. Surface "run `aristo auth login`" hint.
    Auth(crate::auth::AuthError),
    /// Pre-flight network failure (DNS, connect, TLS handshake).
    Network(String),
    /// Server-side request timed out beyond the configured ceiling
    /// (no client-side timeout per §7c row 13; this only fires for
    /// transport-level timeouts the HTTP agent imposes).
    Timeout,
    /// 4xx response. The proxy emits structured `{ "error": "…" }`
    /// bodies for the documented cases:
    ///
    /// - 400 `no_eligible_tags` (every tag filtered out server-side)
    /// - 402 `no_canon_coverage` (plan-empty per §7b)
    /// - 404 `not_found` (GET on a session_id the proxy doesn't know)
    /// - 422 unsupported test_kind
    BadRequest { status: u16, message: String },
    /// 5xx response.
    Server { status: u16, message: String },
    /// Response body didn't deserialize against our wire types.
    /// Indicates an SDK ↔ server contract mismatch.
    Decode(String),
    /// Mock-specific: fixture missing / unloadable. Only surfaces in
    /// tests.
    Fixture(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::Auth(e) => write!(f, "canon-verify auth error: {e}"),
            VerifyError::Network(msg) => write!(f, "canon-verify network error: {msg}"),
            VerifyError::Timeout => write!(f, "canon-verify request timed out"),
            VerifyError::BadRequest { status, message } => {
                write!(f, "canon-verify server returned HTTP {status}: {message}")
            }
            VerifyError::Server { status, message } => {
                write!(f, "canon-verify server error (HTTP {status}): {message}")
            }
            VerifyError::Decode(msg) => write!(f, "canon-verify response decode error: {msg}"),
            VerifyError::Fixture(msg) => write!(f, "canon-verify fixture error: {msg}"),
        }
    }
}

impl std::error::Error for VerifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VerifyError::Auth(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthError;

    #[test]
    fn display_renders_useful_messages() {
        assert!(VerifyError::Auth(AuthError::NoToken)
            .to_string()
            .contains("aristo auth login"));
        assert!(VerifyError::Timeout.to_string().contains("timed out"));
        assert!(VerifyError::BadRequest {
            status: 402,
            message: "no_canon_coverage".into()
        }
        .to_string()
        .contains("402"));
    }
}
