//! End-to-end integration tests for [`HttpCanonClient`] against a
//! local TCP listener. Tests the full transport path — TCP connect,
//! HTTP request framing, response parsing — that the unit tests in
//! `canon::http_client` deliberately skip.
//!
//! The mock server is a tiny single-threaded HTTP/1.1 responder
//! built on `std::net::TcpListener`. Pinning to localhost + an
//! OS-assigned port keeps the tests parallel-safe (no port
//! collision across tests).
//!
//! Each test:
//! 1. Spawns a listener on `127.0.0.1:0` (random free port).
//! 2. Reads the path from the request line, picks a response.
//! 3. Constructs an [`HttpCanonClient`] pointed at the listener's
//!    address.
//! 4. Calls a client method and asserts on the result.
//!
//! These tests **do not** require network access — everything stays
//! on loopback. `cargo test` runs them by default.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use aristo_core::canon::types::{
    AnnotationMatchInput, CanonEntry, CanonMatch, CanonMatchRequest, CanonMatchResponse,
    PrefixTier, References, RequestVerifyBody, RequestVerifyResponse, VerificationMetadata,
};
use aristo_core::canon::{AuthError, CanonClient, CanonError, HttpCanonClient, Token};

/// One canned HTTP/1.1 response. The test driver hands the listener
/// a single response and shuts down.
#[derive(Clone)]
struct CannedResponse {
    status_line: &'static str,
    body: String,
}

/// Spin up a one-shot mock server. Returns `(base_url, JoinHandle)`.
/// The handle blocks the listener thread until one request lands;
/// once the test has called the client, `handle.join()` cleans up.
fn spawn_mock(canned: CannedResponse) -> (String, thread::JoinHandle<MockRecord>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let base = format!("http://{addr}");

    let handle = thread::spawn(move || {
        let (mut stream, _peer) = listener.accept().expect("accept");
        let record = read_request(&mut stream);
        write_response(&mut stream, &canned);
        record
    });
    (base, handle)
}

/// What the mock server saw, so tests can assert on path / headers
/// / body when needed.
#[derive(Debug)]
struct MockRecord {
    method: String,
    path: String,
    /// Authorization header value (raw), if sent.
    authorization: Option<String>,
    body: String,
}

fn read_request(stream: &mut std::net::TcpStream) -> MockRecord {
    let mut reader = BufReader::new(stream.try_clone().expect("clone for read"));
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();

    let mut authorization = None;
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("header line");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        // Header names are case-insensitive per RFC 7230; values
        // are case-sensitive. Match the prefix case-insensitively
        // but preserve the value's original casing.
        if let Some(colon) = line.find(':') {
            let name_lower = line[..colon].to_ascii_lowercase();
            let value = line[colon + 1..].trim().to_string();
            match name_lower.as_str() {
                "authorization" => authorization = Some(value),
                "content-length" => content_length = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).expect("read body");
    }
    MockRecord {
        method,
        path,
        authorization,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn write_response(stream: &mut std::net::TcpStream, canned: &CannedResponse) {
    let body_bytes = canned.body.as_bytes();
    let response = format!(
        "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        canned.status_line,
        body_bytes.len()
    );
    stream.write_all(response.as_bytes()).expect("write head");
    stream.write_all(body_bytes).expect("write body");
    stream.flush().expect("flush");
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

// ─── End-to-end: POST /canon/match happy path ─────────────────────────────

#[test]
fn match_annotations_happy_path_round_trips() {
    let canned_response = CanonMatchResponse {
        results: vec![vec![CanonMatch {
            canon_id: "cell_written_exactly_once_per_page_edit".into(),
            version: "v0.2.1".into(),
            canonical_text: "edit_page writes each cell exactly once".into(),
            confidence: 0.92,
            scope: ":vanilla".into(),
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("specialized neural checker".into()),
            linked: Some("arta_a1b2c3d4".into()),
            verification: VerificationMetadata {
                coverage_level: "tight".into(),
                test_binaries: vec!["monotonicity_property".into()],
            },
        }]],
        effective_scopes: vec![":vanilla".into()],
        canon_version: "v0.2.0".into(),
        matched_at: "2026-06-15T09:14:22Z".into(),
    };
    let body = serde_json::to_string(&canned_response).unwrap();
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 200 OK",
        body,
    });

    let token = Token::new("e2e-test-token");
    let client = HttpCanonClient::new(base, &token);
    let req = CanonMatchRequest {
        annotations: vec![AnnotationMatchInput {
            annotation_text: "each cell should be written exactly once per page edit".into(),
            applies_to: vec!["fn".into()],
        }],
        confidence_threshold: 0.85,
    };
    let resp = client
        .match_annotations(&req)
        .expect("match should succeed");

    // Client-side assertions
    assert_eq!(resp.results.len(), 1);
    assert_eq!(
        resp.results[0][0].canon_id,
        "cell_written_exactly_once_per_page_edit"
    );
    assert_eq!(resp.results[0][0].prefix_tier, PrefixTier::Aristos);

    // Server-side assertions: confirm the SDK actually sent the right shape.
    let record = server.join().expect("server thread");
    assert_eq!(record.method, "POST");
    assert_eq!(record.path, "/canon/match");
    assert_eq!(
        record.authorization.as_deref(),
        Some("Bearer e2e-test-token")
    );
    let sent: CanonMatchRequest = serde_json::from_str(&record.body).expect("sent body is JSON");
    assert_eq!(sent.annotations.len(), 1);
    assert_eq!(
        sent.annotations[0].annotation_text,
        req.annotations[0].annotation_text
    );
    assert!((sent.confidence_threshold - 0.85).abs() < f64::EPSILON);
}

// ─── End-to-end: GET /canon/entry/<id>?version=<v> ────────────────────────

fn aristos_entry(canon_id: &str) -> CanonEntry {
    use std::collections::BTreeMap;
    let mut backed_by = BTreeMap::new();
    backed_by.insert(
        ":vanilla".to_string(),
        Some("specialized neural checker".to_string()),
    );
    let mut prefix_tier_by_scope = BTreeMap::new();
    prefix_tier_by_scope.insert(
        ":vanilla".to_string(),
        aristo_core::canon::PrefixTier::Aristos,
    );
    CanonEntry {
        canon_id: canon_id.to_string(),
        version: "v0.2.1".into(),
        active_version: "v0.2.1".into(),
        is_deprecated: false,
        canon_version: "v0.2.0".into(),
        canonical_text: "the canonical phrasing".into(),
        applies_to: vec!["fn".into()],
        category: "invariants".into(),
        property_type: "safety".into(),
        backed_by,
        prefix_tier_by_scope,
        description: String::new(),
        examples: vec![],
        invariant_sketch: String::new(),
        references: References::default(),
        effective_scopes: vec![":vanilla".into()],
    }
}

fn kanon_entry(canon_id: &str) -> CanonEntry {
    use std::collections::BTreeMap;
    let mut backed_by = BTreeMap::new();
    backed_by.insert(":vanilla".to_string(), None);
    let mut prefix_tier_by_scope = BTreeMap::new();
    prefix_tier_by_scope.insert(
        ":vanilla".to_string(),
        aristo_core::canon::PrefixTier::Kanon,
    );
    CanonEntry {
        canon_id: canon_id.to_string(),
        version: "v0.1.0".into(),
        active_version: "v0.1.0".into(),
        is_deprecated: false,
        canon_version: "v0.1.0".into(),
        canonical_text: "x".into(),
        applies_to: vec!["fn".into()],
        category: "invariants".into(),
        property_type: "safety".into(),
        backed_by,
        prefix_tier_by_scope,
        description: String::new(),
        examples: vec![],
        invariant_sketch: String::new(),
        references: References::default(),
        effective_scopes: vec![":vanilla".into()],
    }
}

#[test]
fn get_entry_with_version_sends_query_param() {
    let canned_entry = aristos_entry("foo");
    let body = serde_json::to_string(&canned_entry).unwrap();
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 200 OK",
        body,
    });

    let token = Token::new("e2e-test-token");
    let client = HttpCanonClient::new(base, &token);
    let entry = client.get_entry("foo", Some("v0.2.1")).expect("get_entry");
    assert_eq!(entry, canned_entry);

    let record = server.join().unwrap();
    assert_eq!(record.method, "GET");
    assert_eq!(record.path, "/canon/entry/foo?version=v0.2.1");
}

#[test]
fn get_entry_without_version_omits_query_param() {
    let canned_entry = kanon_entry("bar");
    let body = serde_json::to_string(&canned_entry).unwrap();
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 200 OK",
        body,
    });

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let _entry = client.get_entry("bar", None).expect("get_entry");

    let record = server.join().unwrap();
    assert_eq!(record.path, "/canon/entry/bar");
}

// ─── End-to-end: POST /canon/request-verify ───────────────────────────────

#[test]
fn request_verify_round_trips_with_optional_note() {
    let canned = RequestVerifyResponse {
        status: "submitted".into(),
        canon_id: "foo".into(),
        current_backing: Some("specialized neural checker".into()),
        previously_submitted_at: None,
    };
    let body = serde_json::to_string(&canned).unwrap();
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 200 OK",
        body,
    });

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let resp = client
        .request_verify(&RequestVerifyBody {
            canon_id: "foo".into(),
            notes: Some("important for our audit".into()),
        })
        .unwrap();
    assert_eq!(resp.status, "submitted");

    let record = server.join().unwrap();
    assert_eq!(record.method, "POST");
    assert_eq!(record.path, "/canon/request-verify");
    let sent: RequestVerifyBody = serde_json::from_str(&record.body).unwrap();
    assert_eq!(sent.canon_id, "foo");
    assert_eq!(sent.notes.as_deref(), Some("important for our audit"));
}

// ─── End-to-end: error status mapping ─────────────────────────────────────

#[test]
fn server_401_maps_to_auth_invalid() {
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 401 Unauthorized",
        body: r#"{"error": "token expired"}"#.into(),
    });

    let token = Token::new("expired-token");
    let client = HttpCanonClient::new(base, &token);
    let err = client
        .match_annotations(&CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.5,
        })
        .unwrap_err();
    assert!(
        matches!(err, CanonError::Auth(AuthError::Invalid)),
        "got {err:?}"
    );
    let _ = server.join();
}

#[test]
fn server_400_carries_message_body() {
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 400 Bad Request",
        body: r#"{"error": "confidence_threshold below floor 0.5"}"#.into(),
    });

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let err = client
        .match_annotations(&CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.3,
        })
        .unwrap_err();
    match err {
        CanonError::BadRequest {
            status: 400,
            message,
        } => {
            assert!(
                message.contains("0.5"),
                "expected message about floor, got {message}"
            );
        }
        other => panic!("expected BadRequest 400, got {other:?}"),
    }
    let _ = server.join();
}

#[test]
fn server_500_maps_to_server_error() {
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 500 Internal Server Error",
        body: r#"{"error": "database connection lost"}"#.into(),
    });

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let err = client
        .match_annotations(&CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.85,
        })
        .unwrap_err();
    match err {
        CanonError::Server {
            status: 500,
            message,
        } => {
            assert!(message.contains("database"));
        }
        other => panic!("expected Server 500, got {other:?}"),
    }
    let _ = server.join();
}

// ─── End-to-end: network failure (no listener) ────────────────────────────

#[test]
fn unreachable_server_maps_to_network_error() {
    // Bind a listener then immediately drop it so the port is
    // released; the SDK call to that port should hit
    // connection-refused. There's a small chance another process
    // grabs the port between drop and connect — accept that and
    // assert on the "fails with Network or Timeout" disjunction
    // rather than the exact variant.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let base = format!("http://{addr}");

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let err = client
        .match_annotations(&CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.85,
        })
        .unwrap_err();
    assert!(
        matches!(err, CanonError::Network(_) | CanonError::Timeout),
        "expected Network or Timeout, got {err:?}"
    );
}

// ─── End-to-end: malformed JSON body ──────────────────────────────────────

#[test]
fn malformed_response_body_maps_to_decode_error() {
    let (base, server) = spawn_mock(CannedResponse {
        status_line: "HTTP/1.1 200 OK",
        body: "this is not JSON".into(),
    });

    let token = Token::new("t");
    let client = HttpCanonClient::new(base, &token);
    let err = client
        .match_annotations(&CanonMatchRequest {
            annotations: vec![],
            confidence_threshold: 0.85,
        })
        .unwrap_err();
    assert!(matches!(err, CanonError::Decode(_)), "got {err:?}");
    let _ = server.join();
}
