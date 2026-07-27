//! [`HttpVerifyClient`] — `ureq`-backed prod impl of [`VerifyClient`].
//!
//! Bearer-token auth using the same `arta_*` token the canon-match
//! client uses ([`crate::auth::resolve_full`] resolves it once at CLI
//! entry; the client clones it into its pre-formatted header).
//!
//! The transport uses a longer per-request timeout than canon-match
//! does (canon-match's 8 s graceful-degradation budget doesn't apply
//! here — verification is a user-initiated long-running task).
//! `?wait=N` server hold-times can extend beyond 8 s when implemented.

use std::time::Duration;

use serde::Serialize;
use ureq::http::Response as HttpResponse;

use super::client::{VerifyClient, VerifyError};
use super::types::{GetVerifySessionResponse, PostVerifySessionResponse, VerifySessionRequest};
use crate::auth::{AuthError, Token};

/// Default base URL — mirrors [`crate::canon::DEFAULT_BASE_URL`].
pub const DEFAULT_BASE_URL: &str = crate::auth::ServerUrl::PROD;

/// Per-request transport timeout in seconds. Wide enough to comfort-
/// ably absorb a 30-second server-side long-poll (§7c row 9) plus
/// network round-trip jitter.
pub const REQUEST_TIMEOUT_SECS: u64 = 60;

/// Per-request timeout for cancel, in seconds. Deliberately short:
/// cancel fires from a signal handler, and CI runners grant only a
/// few seconds between SIGINT and SIGKILL (GitHub Actions: ~7.5 s on
/// `cancel-in-progress`), so waiting out the 60-second poll budget
/// would guarantee the request never leaves the process.
pub const CANCEL_TIMEOUT_SECS: u64 = 5;

/// HTTP-backed [`VerifyClient`].
pub struct HttpVerifyClient {
    base_url: String,
    bearer_header: String,
    agent: ureq::Agent,
}

impl HttpVerifyClient {
    pub fn new(base_url: impl Into<String>, token: &Token) -> Self {
        let base_url = base_url.into();
        let bearer_header = format!("Bearer {}", token.as_str());
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .user_agent(format!("aristo/{}", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .build();
        let agent: ureq::Agent = config.into();
        Self {
            base_url,
            bearer_header,
            agent,
        }
    }

    pub fn production(token: &Token) -> Self {
        Self::new(DEFAULT_BASE_URL, token)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn post_json<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp, VerifyError>
    where
        Req: Serialize,
        Resp: for<'de> serde::Deserialize<'de>,
    {
        let url = self.url(path);
        let result = self
            .agent
            .post(&url)
            .header("Authorization", &self.bearer_header)
            .header("Content-Type", "application/json")
            .send_json(body);
        consume_response(result)
    }

    fn get_json<Resp>(&self, path: &str) -> Result<Resp, VerifyError>
    where
        Resp: for<'de> serde::Deserialize<'de>,
    {
        let url = self.url(path);
        let result = self
            .agent
            .get(&url)
            .header("Authorization", &self.bearer_header)
            .call();
        consume_response(result)
    }
}

impl std::fmt::Debug for HttpVerifyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpVerifyClient")
            .field("base_url", &self.base_url)
            .field("bearer_header", &"Bearer <redacted>")
            .finish()
    }
}

impl VerifyClient for HttpVerifyClient {
    fn post_session(
        &self,
        req: &VerifySessionRequest,
    ) -> Result<PostVerifySessionResponse, VerifyError> {
        self.post_json("/verify/sessions", req)
    }

    fn get_session(
        &self,
        session_id: &str,
        wait_seconds: Option<u32>,
    ) -> Result<GetVerifySessionResponse, VerifyError> {
        let encoded = url_encode(session_id);
        let path = match wait_seconds {
            Some(n) if n > 0 => format!("/verify/sessions/{encoded}?wait={n}"),
            _ => format!("/verify/sessions/{encoded}"),
        };
        self.get_json(&path)
    }

    fn cancel_session(&self, session_id: &str) -> Result<(), VerifyError> {
        let url = self.url(&cancel_path(session_id));
        // Same agent (keeps the connection pool), but a short
        // per-request timeout override — see [`CANCEL_TIMEOUT_SECS`].
        let result = self
            .agent
            .post(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(CANCEL_TIMEOUT_SECS)))
            .build()
            .header("Authorization", &self.bearer_header)
            .send_empty();
        match result {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                if (200..300).contains(&status) {
                    // 202 carries a session snapshot; the caller only
                    // needs "the server took the cancel", so the body
                    // is deliberately not decoded.
                    return Ok(());
                }
                let body = resp.body_mut().read_to_string().unwrap_or_default();
                Err(error_for_status(status, &body))
            }
            Err(e) => Err(transport_error_to_verify_error(e)),
        }
    }
}

/// Path for `POST /verify/sessions/:id/cancel` (relative to the
/// data-plane base URL).
fn cancel_path(session_id: &str) -> String {
    format!("/verify/sessions/{}/cancel", url_encode(session_id))
}

// ─── Pure response mapping (mirror of canon http_client) ──────────────────

fn consume_response<T>(
    result: Result<HttpResponse<ureq::Body>, ureq::Error>,
) -> Result<T, VerifyError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    match result {
        Ok(mut resp) => {
            let status = resp.status().as_u16();
            let body = resp
                .body_mut()
                .read_to_string()
                .map_err(|e| VerifyError::Decode(format!("read body: {e}")))?;
            map_response(status, &body)
        }
        Err(e) => Err(transport_error_to_verify_error(e)),
    }
}

pub(crate) fn map_response<T>(status: u16, body: &str) -> Result<T, VerifyError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    match status {
        200..=299 => serde_json::from_str(body)
            .map_err(|e| VerifyError::Decode(format!("parse 2xx body: {e}"))),
        _ => Err(error_for_status(status, body)),
    }
}

/// The [`VerifyError`] for a non-2xx response. Shared between
/// [`map_response`] (body-decoding endpoints) and `cancel_session`
/// (which ignores its success body).
pub(crate) fn error_for_status(status: u16, body: &str) -> VerifyError {
    match status {
        401 => VerifyError::Auth(AuthError::Invalid),
        400..=499 => VerifyError::BadRequest {
            status,
            message: extract_message_or_body(body),
        },
        500..=599 => VerifyError::Server {
            status,
            message: extract_message_or_body(body),
        },
        other => VerifyError::Server {
            status: other,
            message: format!("unexpected status code {other}"),
        },
    }
}

pub(crate) fn transport_error_to_verify_error(e: ureq::Error) -> VerifyError {
    let s = e.to_string();
    if s.contains("timed out") || s.contains("timeout") {
        VerifyError::Timeout
    } else {
        VerifyError::Network(s)
    }
}

fn extract_message_or_body(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(s) = v.get("error").and_then(|x| x.as_str()) {
            return s.to_string();
        }
        if let Some(s) = v.get("message").and_then(|x| x.as_str()) {
            return s.to_string();
        }
    }
    let trimmed = body.trim();
    if trimmed.len() > 500 {
        format!("{}…", &trimmed[..500])
    } else {
        trimmed.to_string()
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let safe = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon_verify::types::VerifySessionTag;

    fn sample_post_response_json() -> String {
        let resp = PostVerifySessionResponse {
            session_id: "01HN1234567890".into(),
            view_url: "https://dev.aretta.ai/dashboard/jobs/01HN1234567890".into(),
            plan_size: 2,
        };
        serde_json::to_string(&resp).unwrap()
    }

    #[test]
    fn map_response_2xx_decodes_post() {
        let body = sample_post_response_json();
        let resp: PostVerifySessionResponse = map_response(202, &body).unwrap();
        assert_eq!(resp.session_id, "01HN1234567890");
        assert_eq!(resp.plan_size, 2);
    }

    #[test]
    fn map_response_2xx_garbage_is_decode_error() {
        let err: Result<PostVerifySessionResponse, _> = map_response(200, "not json");
        assert!(matches!(err.unwrap_err(), VerifyError::Decode(_)));
    }

    #[test]
    fn map_response_401_maps_to_auth_invalid() {
        let err: Result<PostVerifySessionResponse, _> = map_response(401, "{}");
        assert!(matches!(
            err.unwrap_err(),
            VerifyError::Auth(AuthError::Invalid)
        ));
    }

    #[test]
    fn map_response_402_no_canon_coverage_is_bad_request_with_message() {
        let err: Result<PostVerifySessionResponse, _> = map_response(
            402,
            r#"{"error": "no_canon_coverage", "message": "no canon coverage applies for your scopes"}"#,
        );
        match err.unwrap_err() {
            VerifyError::BadRequest { status, message } => {
                assert_eq!(status, 402);
                assert!(message.contains("no_canon_coverage"));
            }
            other => panic!("expected BadRequest 402, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_no_eligible_tags() {
        let err: Result<PostVerifySessionResponse, _> =
            map_response(400, r#"{"error": "no_eligible_tags"}"#);
        match err.unwrap_err() {
            VerifyError::BadRequest { status, message } => {
                assert_eq!(status, 400);
                assert!(message.contains("no_eligible_tags"));
            }
            other => panic!("expected BadRequest 400, got {other:?}"),
        }
    }

    #[test]
    fn map_response_404_for_get_session() {
        let err: Result<GetVerifySessionResponse, _> =
            map_response(404, r#"{"error": "not_found"}"#);
        match err.unwrap_err() {
            VerifyError::BadRequest {
                status: 404,
                message,
            } => {
                assert!(message.contains("not_found"));
            }
            other => panic!("expected BadRequest 404, got {other:?}"),
        }
    }

    #[test]
    fn map_response_5xx_is_server_error() {
        let err: Result<PostVerifySessionResponse, _> =
            map_response(503, r#"{"error": "upstream timeout"}"#);
        match err.unwrap_err() {
            VerifyError::Server {
                status: 503,
                message,
            } => {
                assert!(message.contains("upstream"));
            }
            other => panic!("expected Server 503, got {other:?}"),
        }
    }

    #[test]
    fn http_client_construction_does_not_panic() {
        let tok = Token::new("test-token");
        let c = HttpVerifyClient::new("https://example.test", &tok);
        assert_eq!(c.base_url, "https://example.test");
        assert_eq!(c.bearer_header, "Bearer test-token");
    }

    #[test]
    fn http_client_debug_redacts_token() {
        let tok = Token::new("super-secret-do-not-log");
        let c = HttpVerifyClient::new("https://example.test", &tok);
        let s = format!("{c:?}");
        assert!(
            !s.contains("super-secret-do-not-log"),
            "Debug must not leak token: {s}"
        );
        assert!(s.contains("redacted"));
    }

    #[test]
    fn http_client_is_send_and_object_safe() {
        let tok = Token::new("t");
        let _: Box<dyn VerifyClient> =
            Box::new(HttpVerifyClient::new("https://example.test", &tok));
    }

    #[test]
    fn cancel_path_targets_the_cancel_endpoint_with_encoding() {
        assert_eq!(
            cancel_path("01HN1234567890"),
            "/verify/sessions/01HN1234567890/cancel"
        );
        // Defensive: a hostile session id can't smuggle path segments.
        assert_eq!(cancel_path("a/b"), "/verify/sessions/a%2Fb/cancel");
    }

    #[test]
    fn error_for_status_mirrors_map_response_classes() {
        assert!(matches!(
            error_for_status(401, "{}"),
            VerifyError::Auth(AuthError::Invalid)
        ));
        assert!(matches!(
            error_for_status(409, r#"{"error": "conflict"}"#),
            VerifyError::BadRequest { status: 409, .. }
        ));
        assert!(matches!(
            error_for_status(503, "{}"),
            VerifyError::Server { status: 503, .. }
        ));
    }

    #[test]
    fn url_encode_handles_session_id_charset() {
        // Session ids are UUIDv4-ish — alphanumeric + hyphen. All safe.
        assert_eq!(
            url_encode("01HN1234567890ABCDEFGHJKMN"),
            "01HN1234567890ABCDEFGHJKMN"
        );
        assert_eq!(
            url_encode("a1b2c3d4-e5f6-7890-1234-567890abcdef"),
            "a1b2c3d4-e5f6-7890-1234-567890abcdef"
        );
        // Defensive: anything else escapes.
        assert_eq!(url_encode("foo bar"), "foo%20bar");
    }

    // Compile-time / dummy use to keep VerifySessionRequest reachable
    // from this module (a downstream contract change should ripple).
    #[allow(dead_code)]
    fn _request_shape_is_reachable() {
        let _ = VerifySessionRequest {
            repo_full_name: String::new(),
            commit_sha: String::new(),
            tags: vec![VerifySessionTag {
                annotation_id: String::new(),
                canon_id: String::new(),
                version: String::new(),
                source_path: String::new(),
            }],
        };
    }
}
