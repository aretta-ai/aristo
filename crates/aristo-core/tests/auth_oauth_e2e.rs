//! End-to-end integration tests for the GitHub OAuth login flow
//! against a local TCP listener. Same pattern as `canon_http_e2e.rs`.
//!
//! Each test spawns a one-shot HTTP/1.1 mock server on
//! `127.0.0.1:0` (OS-assigned port), points `ServerUrl::Custom(...)`
//! at the listener address, and exercises `oauth_start` or
//! `oauth_exchange`. Tests **do not** require network access —
//! everything stays on loopback.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use aristo_core::auth::{
    oauth_exchange, oauth_start, AuthError, CliTokenResponse, OAuthInit, ServerUrl,
};

#[derive(Clone)]
struct CannedResponse {
    status_line: &'static str,
    body: String,
}

#[derive(Debug)]
struct MockRecord {
    method: String,
    path: String,
    body: String,
}

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

fn read_request(stream: &mut std::net::TcpStream) -> MockRecord {
    let mut reader = BufReader::new(stream.try_clone().expect("clone for read"));
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("header line");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        let (k, v) = match line.split_once(':') {
            Some(kv) => kv,
            None => continue,
        };
        if k.trim().eq_ignore_ascii_case("content-length") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).expect("body bytes");
    }
    MockRecord {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn write_response(stream: &mut std::net::TcpStream, canned: &CannedResponse) {
    let payload = format!(
        "HTTP/1.1 {sl}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
        sl = canned.status_line,
        n = canned.body.len(),
        body = canned.body,
    );
    let _ = stream.write_all(payload.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn custom_server(base: &str) -> ServerUrl {
    ServerUrl::Custom(base.to_string())
}

// ─── oauth_start ───────────────────────────────────────────────────────────

#[test]
fn oauth_start_decodes_authorize_url_from_proxy_response() {
    let (base, handle) = spawn_mock(CannedResponse {
        status_line: "200 OK",
        body: r#"{"url":"https://github.com/login/oauth/authorize?client_id=abc&redirect_uri=foo&scope=read:user"}"#
            .into(),
    });
    let result: OAuthInit = oauth_start(&custom_server(&base)).expect("ok");
    assert!(
        result
            .authorize_url
            .starts_with("https://github.com/login/oauth/authorize"),
        "got: {}",
        result.authorize_url
    );
    assert!(
        result.authorize_url.contains("client_id=abc"),
        "got: {}",
        result.authorize_url
    );

    let record = handle.join().unwrap();
    assert_eq!(record.method, "GET");
    assert_eq!(record.path, "/auth/login");
}

#[test]
fn oauth_start_5xx_surfaces_malformed() {
    let (base, _handle) = spawn_mock(CannedResponse {
        status_line: "503 Service Unavailable",
        body: r#"{"error":"backend offline"}"#.into(),
    });
    let err = oauth_start(&custom_server(&base)).expect_err("must fail");
    match err {
        AuthError::Malformed(m) => {
            assert!(m.contains("503"), "got: {m}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

// ─── oauth_exchange ────────────────────────────────────────────────────────

#[test]
fn oauth_exchange_happy_path_returns_arta_token_and_user() {
    let body = r#"{
        "arta_token": "arta_test1234567890",
        "jwt": "ey.jwt.blob",
        "user": { "id": 42, "login": "octocat" },
        "token_id": "tok_xyz",
        "repo_full_name": "owner/repo",
        "last_4": "7890"
    }"#;
    let (base, handle) = spawn_mock(CannedResponse {
        status_line: "200 OK",
        body: body.into(),
    });
    let r: CliTokenResponse = oauth_exchange(
        &custom_server(&base),
        "oauth-code-abc",
        "owner/repo",
        Some("aristo-cli"),
    )
    .expect("ok");
    assert_eq!(r.arta_token, "arta_test1234567890");
    assert_eq!(r.user.login, "octocat");
    assert_eq!(r.user.id, 42);
    assert_eq!(r.repo_full_name, "owner/repo");
    assert_eq!(r.last_4, "7890");

    // Verify the request shape: POST /auth/cli-token with JSON body.
    let record = handle.join().unwrap();
    assert_eq!(record.method, "POST");
    assert_eq!(record.path, "/auth/cli-token");
    // ureq sends the JSON pretty-printed; assert by parsing back.
    let parsed: serde_json::Value =
        serde_json::from_str(&record.body).expect("body parses as JSON");
    assert_eq!(parsed["code"], "oauth-code-abc");
    assert_eq!(parsed["repoFullName"], "owner/repo");
    assert_eq!(parsed["name"], "aristo-cli");
}

#[test]
fn oauth_exchange_omits_name_when_none() {
    let body = r#"{
        "arta_token": "arta_x",
        "jwt": "j",
        "user": { "id": 1, "login": "u" },
        "token_id": "t",
        "repo_full_name": "o/r",
        "last_4": "abcd"
    }"#;
    let (base, handle) = spawn_mock(CannedResponse {
        status_line: "200 OK",
        body: body.into(),
    });
    let _ = oauth_exchange(&custom_server(&base), "c", "o/r", None).expect("ok");
    let record = handle.join().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&record.body).expect("body parses as JSON");
    // `name` is skip_serializing_if::Option::is_none.
    assert!(
        parsed.get("name").is_none(),
        "expected name omitted; got body: {}",
        record.body
    );
    assert_eq!(parsed["code"], "c");
    assert_eq!(parsed["repoFullName"], "o/r");
}

#[test]
fn oauth_exchange_400_missing_code_surfaces_proxy_error_message() {
    let (base, _handle) = spawn_mock(CannedResponse {
        status_line: "400 Bad Request",
        body: r#"{"error":"Missing code"}"#.into(),
    });
    let err = oauth_exchange(&custom_server(&base), "", "owner/repo", None).expect_err("must fail");
    match err {
        AuthError::Malformed(m) => assert!(m.contains("Missing code"), "got: {m}"),
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn oauth_exchange_403_unknown_user_maps_to_invalid() {
    let (base, _handle) = spawn_mock(CannedResponse {
        status_line: "403 Forbidden",
        body: r#"{"error":"User not authorized"}"#.into(),
    });
    let err =
        oauth_exchange(&custom_server(&base), "c", "owner/repo", None).expect_err("must fail");
    assert_eq!(err, AuthError::Invalid);
}

#[test]
fn oauth_exchange_502_oauth_failed_maps_to_malformed_with_proxy_message() {
    let (base, _handle) = spawn_mock(CannedResponse {
        status_line: "502 Bad Gateway",
        body: r#"{"error":"OAuth exchange failed: github unreachable"}"#.into(),
    });
    let err =
        oauth_exchange(&custom_server(&base), "c", "owner/repo", None).expect_err("must fail");
    match err {
        AuthError::Malformed(m) => {
            assert!(m.contains("502"), "got: {m}");
            assert!(m.contains("OAuth exchange failed"), "got: {m}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}
