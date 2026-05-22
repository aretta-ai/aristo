//! [`HttpCanonClient`] — production canon API client via `ureq`.
//!
//! Maps HTTP responses to [`CanonError`] per the graceful-degradation
//! contract in `../aretta-sdk/docs/mockups/13-canon-and-matching/README.md`
//! §L3:
//!
//! - **2-second timeout** → [`CanonError::Timeout`]
//! - **DNS / connect failure** → [`CanonError::Network`]
//! - **401** → [`CanonError::Auth(AuthError::Invalid)`]
//! - **400** (e.g., confidence threshold below `0.5` floor) →
//!   [`CanonError::BadRequest`]
//! - **Other 4xx** → [`CanonError::BadRequest`] (the message body
//!   carries the specific reason)
//! - **5xx** → [`CanonError::Server`]
//! - **Body parse failure on a 2xx** → [`CanonError::Decode`]
//!
//! The transport (`ureq`) and the response-mapping logic
//! ([`map_response`] et al) are split so the response-mapping
//! tests cover every status-code branch exhaustively without
//! spinning up a server.

use std::time::Duration;

use serde::Serialize;
use ureq::http::Response as HttpResponse;

use super::client::{AuthError, CanonClient, CanonError};
use super::types::{
    CanonEntry, CanonMatchRequest, CanonMatchResponse, RequestVerifyBody, RequestVerifyResponse,
};
use super::Token;

/// Default base URL for the canon API. Points at production
/// (`code.aretta.ai`); tests + staging override via
/// [`HttpCanonClient::new`]. See [`crate::auth::ServerUrl`] for the
/// dev/prod/custom enum that owns the well-known URL constants.
pub const DEFAULT_BASE_URL: &str = crate::auth::ServerUrl::PROD;

/// L3 graceful-degradation timeout. Applies to each individual
/// request (not the whole operation).
pub const REQUEST_TIMEOUT_SECS: u64 = 2;

/// HTTP-backed [`CanonClient`] impl.
///
/// One agent is shared across calls (connection reuse for the
/// stamp-then-critique pattern where two close-spaced calls are
/// common). Tokens are passed by reference at construction; the
/// client clones them per request so the underlying string stays
/// owned by the client.
pub struct HttpCanonClient {
    base_url: String,
    bearer_header: String,
    agent: ureq::Agent,
}

impl HttpCanonClient {
    /// Construct a client. `base_url` should NOT end with `/` —
    /// the client appends absolute paths (`/canon/match` etc.).
    pub fn new(base_url: impl Into<String>, token: &Token) -> Self {
        let base_url = base_url.into();
        // Pre-compute the Bearer header so we don't reformat per
        // call. The token string is also embedded here; the original
        // Token's redacted Debug impl prevents leaks via the client's
        // Debug.
        let bearer_header = format!("Bearer {}", token.as_str());

        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .user_agent(format!("aristo/{}", env!("CARGO_PKG_VERSION")))
            // Treat non-2xx as Ok(Response) so the SDK's
            // map_response handles each status code uniformly.
            // ureq's default surfaces 4xx/5xx as errors, which would
            // mean two error-mapping paths for the same data — keep
            // the dispatch in one place.
            .http_status_as_error(false)
            .build();
        let agent: ureq::Agent = config.into();

        Self {
            base_url,
            bearer_header,
            agent,
        }
    }

    /// Construct with the production base URL.
    pub fn production(token: &Token) -> Self {
        Self::new(DEFAULT_BASE_URL, token)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn post_json<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp, CanonError>
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

    fn get_json<Resp>(&self, path: &str) -> Result<Resp, CanonError>
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

impl std::fmt::Debug for HttpCanonClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact bearer_header so Debug never prints the token.
        f.debug_struct("HttpCanonClient")
            .field("base_url", &self.base_url)
            .field("bearer_header", &"Bearer <redacted>")
            .finish()
    }
}

impl CanonClient for HttpCanonClient {
    fn match_annotations(&self, req: &CanonMatchRequest) -> Result<CanonMatchResponse, CanonError> {
        self.post_json("/canon/match", req)
    }

    fn get_entry(&self, canon_id: &str, version: Option<&str>) -> Result<CanonEntry, CanonError> {
        let canon_id = url_encode(canon_id);
        let path = match version {
            Some(v) => format!("/canon/entry/{canon_id}?version={}", url_encode(v)),
            None => format!("/canon/entry/{canon_id}"),
        };
        self.get_json(&path)
    }

    fn request_verify(
        &self,
        body: &RequestVerifyBody,
    ) -> Result<RequestVerifyResponse, CanonError> {
        self.post_json("/canon/request-verify", body)
    }
}

// ─── Pure response-mapping helpers ─────────────────────────────────────────

/// Drive a `ureq::Result<HttpResponse<ureq::Body>>` to either a
/// decoded body of type `T` or a [`CanonError`]. Split out from
/// the transport so each error branch is unit-testable without a
/// real network call.
fn consume_response<T>(
    result: Result<HttpResponse<ureq::Body>, ureq::Error>,
) -> Result<T, CanonError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    match result {
        Ok(mut resp) => {
            let status = resp.status().as_u16();
            let body = resp
                .body_mut()
                .read_to_string()
                .map_err(|e| CanonError::Decode(format!("read body: {e}")))?;
            map_response(status, &body)
        }
        Err(e) => Err(transport_error_to_canon_error(e)),
    }
}

/// Convert a status + body into either a decoded `T` or a typed
/// [`CanonError`]. Pure function — no I/O.
pub(crate) fn map_response<T>(status: u16, body: &str) -> Result<T, CanonError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    match status {
        200..=299 => serde_json::from_str(body)
            .map_err(|e| CanonError::Decode(format!("parse 2xx body: {e}"))),
        401 => Err(CanonError::Auth(AuthError::Invalid)),
        400..=499 => Err(CanonError::BadRequest {
            status,
            message: extract_message_or_body(body),
        }),
        500..=599 => Err(CanonError::Server {
            status,
            message: extract_message_or_body(body),
        }),
        // 1xx / 3xx shouldn't reach here — ureq follows redirects.
        // If we see one anyway, treat it as a server-side bug.
        other => Err(CanonError::Server {
            status: other,
            message: format!("unexpected status code {other}"),
        }),
    }
}

/// Turn a `ureq::Error` into the appropriate [`CanonError`]. Status
/// errors are now part of `Ok(Response)` in ureq 3.x; this function
/// handles transport-only failures (network, timeout, TLS).
pub(crate) fn transport_error_to_canon_error(e: ureq::Error) -> CanonError {
    // Match on the textual form because ureq 3.x's Error variants
    // are non-exhaustive and we want forward compatibility. The
    // Display-form of each variant is reliable; we slice it to
    // route into the right CanonError bucket. Anything that isn't
    // recognizably a timeout falls into Network with the original
    // message preserved.
    let s = e.to_string();
    if s.contains("timed out") || s.contains("timeout") {
        CanonError::Timeout
    } else {
        CanonError::Network(s)
    }
}

/// Best-effort: pull a JSON `{"error": "..."}` or `{"message":
/// "..."}` value out of the body for user-facing display. If the
/// body isn't JSON or doesn't have those keys, return the body
/// verbatim (truncated to keep error messages reasonable).
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

/// Minimal URL-path encoder for canon ids and version strings.
/// Canon ids are constrained to `[a-z0-9_]`; versions to
/// `v<digits>.<digits>.<digits>`. Both are ASCII; we still escape
/// defensively in case the server ever issues an id with a special
/// character.
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
    use crate::canon::types::{
        AnnotationMatchInput, CanonMatch, PrefixTier, References, VerificationMetadata,
    };

    // ─── map_response: status-code dispatch ────────────────────────────────

    fn sample_match_response_json() -> String {
        let resp = CanonMatchResponse {
            results: vec![vec![CanonMatch {
                canon_id: "foo".into(),
                version: "v0.1.0".into(),
                canonical_text: "foo".into(),
                confidence: 0.9,
                scope: ":vanilla".into(),
                prefix_tier: PrefixTier::Kanon,
                backed_by: None,
                linked: Some("arta_xyz".into()),
                verification: VerificationMetadata {
                    coverage_level: "none".into(),
                    test_binaries: vec![],
                },
            }]],
            effective_scopes: vec![":vanilla".into()],
            canon_version: "v0.2.0".into(),
            matched_at: "2026-06-15T09:14:22Z".into(),
        };
        serde_json::to_string(&resp).unwrap()
    }

    #[test]
    fn map_response_200_decodes_match_response() {
        let body = sample_match_response_json();
        let resp: CanonMatchResponse = map_response(200, &body).unwrap();
        assert_eq!(resp.canon_version, "v0.2.0");
        assert_eq!(resp.results[0][0].canon_id, "foo");
    }

    #[test]
    fn map_response_2xx_other_codes_also_decode() {
        // 201 / 202 / 204 should all decode the body the same way.
        let body = sample_match_response_json();
        let resp: CanonMatchResponse = map_response(201, &body).unwrap();
        assert_eq!(resp.canon_version, "v0.2.0");
        let resp: CanonMatchResponse = map_response(299, &body).unwrap();
        assert_eq!(resp.canon_version, "v0.2.0");
    }

    #[test]
    fn map_response_2xx_garbage_body_is_decode_error() {
        let err: Result<CanonMatchResponse, _> = map_response(200, "not json");
        let err = err.unwrap_err();
        assert!(matches!(err, CanonError::Decode(_)));
    }

    #[test]
    fn map_response_401_maps_to_auth_invalid() {
        let err: Result<CanonMatchResponse, _> = map_response(401, "{}");
        let err = err.unwrap_err();
        assert!(matches!(err, CanonError::Auth(AuthError::Invalid)));
    }

    #[test]
    fn map_response_400_with_json_error_field_extracts_message() {
        let body = r#"{"error": "confidence_threshold below floor 0.5"}"#;
        let err: Result<CanonMatchResponse, _> = map_response(400, body);
        let err = err.unwrap_err();
        match err {
            CanonError::BadRequest { status, message } => {
                assert_eq!(status, 400);
                assert!(message.contains("0.5"));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_with_json_message_field_extracts_message() {
        let body = r#"{"message": "missing annotations"}"#;
        let err: Result<CanonMatchResponse, _> = map_response(400, body);
        match err.unwrap_err() {
            CanonError::BadRequest {
                status: 400,
                message,
            } => {
                assert!(message.contains("missing"));
            }
            other => panic!("expected BadRequest 400, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_with_plain_text_passes_through_body() {
        let err: Result<CanonMatchResponse, _> = map_response(400, "raw text reason");
        match err.unwrap_err() {
            CanonError::BadRequest {
                status: 400,
                message,
            } => {
                assert_eq!(message, "raw text reason");
            }
            other => panic!("expected BadRequest 400, got {other:?}"),
        }
    }

    #[test]
    fn map_response_404_for_get_entry() {
        let err: Result<CanonEntry, _> = map_response(404, r#"{"error": "canon entry not found"}"#);
        match err.unwrap_err() {
            CanonError::BadRequest {
                status: 404,
                message,
            } => {
                assert!(message.contains("not found"));
            }
            other => panic!("expected BadRequest 404, got {other:?}"),
        }
    }

    #[test]
    fn map_response_500_maps_to_server_error() {
        let err: Result<CanonMatchResponse, _> =
            map_response(500, r#"{"error": "internal server bug"}"#);
        match err.unwrap_err() {
            CanonError::Server {
                status: 500,
                message,
            } => {
                assert!(message.contains("internal"));
            }
            other => panic!("expected Server 500, got {other:?}"),
        }
    }

    #[test]
    fn map_response_503_maps_to_server_error() {
        let err: Result<CanonMatchResponse, _> = map_response(503, "");
        assert!(matches!(
            err.unwrap_err(),
            CanonError::Server { status: 503, .. }
        ));
    }

    #[test]
    fn map_response_truncates_huge_body() {
        let huge = "x".repeat(2000);
        let err: Result<CanonMatchResponse, _> = map_response(400, &huge);
        match err.unwrap_err() {
            CanonError::BadRequest { message, .. } => {
                assert!(
                    message.len() < 1000,
                    "expected truncation, got {} chars",
                    message.len()
                );
                assert!(message.ends_with('…'));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // ─── url_encode ────────────────────────────────────────────────────────

    #[test]
    fn url_encode_passes_through_safe_chars() {
        assert_eq!(url_encode("foo_bar123"), "foo_bar123");
        assert_eq!(url_encode("v0.2.1"), "v0.2.1");
        assert_eq!(url_encode("a-b~c"), "a-b~c");
    }

    #[test]
    fn url_encode_escapes_special_chars() {
        assert_eq!(url_encode("foo bar"), "foo%20bar");
        assert_eq!(url_encode("foo:bar"), "foo%3Abar");
        assert_eq!(url_encode("foo&bar=baz"), "foo%26bar%3Dbaz");
        assert_eq!(url_encode("foo/bar"), "foo%2Fbar");
    }

    // ─── HttpCanonClient construction ─────────────────────────────────────

    #[test]
    fn http_client_construction_does_not_panic() {
        // Bare smoke test: construction shouldn't make any network
        // calls. Real request-path coverage lives in
        // tests/canon_http_e2e.rs against a localhost listener.
        let tok = Token::new("test-token");
        let c = HttpCanonClient::new("https://example.test", &tok);
        assert_eq!(c.base_url, "https://example.test");
        // Bearer header is pre-computed.
        assert_eq!(c.bearer_header, "Bearer test-token");
    }

    #[test]
    fn http_client_debug_redacts_token() {
        let tok = Token::new("super-secret-do-not-log");
        let c = HttpCanonClient::new("https://example.test", &tok);
        let s = format!("{c:?}");
        assert!(
            !s.contains("super-secret-do-not-log"),
            "Debug must not leak token: {s}"
        );
        assert!(s.contains("redacted"));
    }

    #[test]
    fn http_client_url_construction() {
        let tok = Token::new("t");
        let c = HttpCanonClient::new("https://api.example.test", &tok);
        assert_eq!(
            c.url("/canon/match"),
            "https://api.example.test/canon/match"
        );
        assert_eq!(
            c.url("/canon/entry/foo"),
            "https://api.example.test/canon/entry/foo"
        );
    }

    #[test]
    fn http_client_production_constructor_uses_default_base_url() {
        let tok = Token::new("t");
        let c = HttpCanonClient::production(&tok);
        assert_eq!(c.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn http_client_is_send_and_object_safe() {
        // Compile-time check: HttpCanonClient must be usable behind
        // Box<dyn CanonClient>. The trait requires Send + Sync;
        // ureq::Agent is Send + Sync as of 2.x / 3.x.
        let tok = Token::new("t");
        let _boxed: Box<dyn CanonClient> =
            Box::new(HttpCanonClient::new("https://example.test", &tok));
    }

    // ─── Sanity: real types round-trip through map_response ───────────────

    #[test]
    fn map_response_round_trip_via_canon_entry() {
        let entry = CanonEntry {
            id: "foo".into(),
            version: "v0.2.1".into(),
            canonical_text: "the canonical phrasing".into(),
            applies_to: vec!["fn".into()],
            category: "invariants".into(),
            property_type: "safety".into(),
            scope: ":vanilla".into(),
            backed_by: Some("specialized neural checker".into()),
            description: None,
            examples: vec![],
            references: References::default(),
        };
        let body = serde_json::to_string(&entry).unwrap();
        let got: CanonEntry = map_response(200, &body).unwrap();
        assert_eq!(got, entry);
    }

    #[test]
    fn map_response_round_trip_via_request_verify_response() {
        let resp = RequestVerifyResponse {
            status: "submitted".into(),
            canon_id: "foo".into(),
            current_backing: None,
            previously_submitted_at: None,
        };
        let body = serde_json::to_string(&resp).unwrap();
        let got: RequestVerifyResponse = map_response(200, &body).unwrap();
        assert_eq!(got, resp);
    }

    // The unused-by-CanonClient method `unused_match_request` is
    // gated here only to ensure the imports in this file stay tied
    // to a real use site.
    #[allow(dead_code)]
    fn _import_match_request_for_test() {
        let _req = CanonMatchRequest {
            annotations: vec![AnnotationMatchInput {
                annotation_text: "x".into(),
                applies_to: vec!["fn".into()],
            }],
            confidence_threshold: 0.85,
        };
    }
}
