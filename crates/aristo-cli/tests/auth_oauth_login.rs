//! End-to-end test for `aristo auth login` in OAuth mode.
//!
//! Pattern: spawn a one-shot TCP mock server, run the real `aristo`
//! binary pointed at it via `--server http://127.0.0.1:<port>`, pipe
//! the OAuth code into stdin, and assert that the credentials file
//! ends up containing the token returned by the mock.
//!
//! Two-request flow:
//! 1. `GET /auth/login` — returns the GitHub URL.
//! 2. `POST /auth/cli-token` with the code + repoFullName — returns
//!    the `arta_*` token.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use tempfile::TempDir;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Build an isolated `Command` running `aristo` from a sandbox: clean
/// env, isolated `HOME`/`XDG_CONFIG_HOME` so the user's real
/// credentials are never touched.
fn isolated(workspace: &Path) -> Command {
    let mut c = Command::new(aristo_bin());
    c.env_clear();
    if let Ok(p) = std::env::var("PATH") {
        c.env("PATH", p);
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
        c.env("DYLD_FALLBACK_LIBRARY_PATH", p);
    }
    let home = workspace.join("home");
    std::fs::create_dir_all(&home).unwrap();
    c.env("HOME", &home);
    c.env("XDG_CONFIG_HOME", home.join("xdg"));
    c.current_dir(workspace);
    c
}

/// Spin up a two-step mock server that:
///
/// 1. Serves the next `GET /auth/login` with `login_response`.
/// 2. Then serves the next `POST /auth/cli-token` with `token_response`.
///
/// Returns `(base_url, JoinHandle)`. Handle joins after both
/// requests have been seen.
fn spawn_two_step_mock(
    login_response: CannedResponse,
    token_response: CannedResponse,
) -> (String, thread::JoinHandle<(MockRecord, MockRecord)>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let base = format!("http://{addr}");

    let handle = thread::spawn(move || {
        let (mut stream1, _) = listener.accept().expect("accept #1");
        let rec1 = read_request(&mut stream1);
        write_response(&mut stream1, &login_response);

        let (mut stream2, _) = listener.accept().expect("accept #2");
        let rec2 = read_request(&mut stream2);
        write_response(&mut stream2, &token_response);
        (rec1, rec2)
    });

    (base, handle)
}

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

#[test]
fn auth_login_oauth_persists_arta_token_on_happy_path() {
    let workspace = TempDir::new().unwrap();

    let (base, handle) = spawn_two_step_mock(
        CannedResponse {
            status_line: "200 OK",
            body: r#"{"url":"https://github.com/login/oauth/authorize?client_id=test"}"#.into(),
        },
        CannedResponse {
            status_line: "200 OK",
            body: r#"{
                "arta_token": "arta_e2etest1234",
                "jwt": "jwt-blob",
                "user": { "id": 42, "login": "octocat" },
                "token_id": "tok_1",
                "repo_full_name": "owner/repo",
                "last_4": "1234"
            }"#
            .into(),
        },
    );

    let mut cmd = isolated(workspace.path());
    cmd.args(["auth", "login", "--server", &base, "--repo", "owner/repo"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn aristo");
    // Pipe the OAuth code into stdin.
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(b"oauth-code-from-callback\n").unwrap();
    }
    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "auth login failed: stdout={stdout}\nstderr={stderr}"
    );

    let (rec1, rec2) = handle.join().unwrap();
    assert_eq!(rec1.method, "GET");
    assert_eq!(rec1.path, "/auth/login");
    assert_eq!(rec2.method, "POST");
    assert_eq!(rec2.path, "/auth/cli-token");
    let body: serde_json::Value = serde_json::from_str(&rec2.body).expect("body parses as JSON");
    assert_eq!(body["code"], "oauth-code-from-callback");
    assert_eq!(body["repoFullName"], "owner/repo");

    // The credentials file should contain the arta_token returned by
    // the mock.
    let creds_path = workspace.path().join("home/xdg/aristo/credentials");
    assert!(
        creds_path.exists(),
        "credentials file should exist at {}",
        creds_path.display()
    );
    let body = std::fs::read_to_string(&creds_path).unwrap();
    assert!(
        body.contains("arta_e2etest1234"),
        "credentials should contain the arta_token; got:\n{body}"
    );

    // User-facing stdout should include the login + repo for confirmation.
    assert!(
        stdout.contains("octocat"),
        "expected user login in stdout; got: {stdout}"
    );
    assert!(
        stdout.contains("owner/repo"),
        "expected repo in stdout; got: {stdout}"
    );
}

#[test]
fn auth_login_oauth_403_unknown_user_surfaces_invalid_error() {
    let workspace = TempDir::new().unwrap();

    let (base, _handle) = spawn_two_step_mock(
        CannedResponse {
            status_line: "200 OK",
            body: r#"{"url":"https://github.com/login/oauth/authorize?client_id=x"}"#.into(),
        },
        CannedResponse {
            status_line: "403 Forbidden",
            body: r#"{"error":"User not authorized"}"#.into(),
        },
    );

    let mut cmd = isolated(workspace.path());
    cmd.args(["auth", "login", "--server", &base, "--repo", "owner/repo"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn");
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"some-code\n").unwrap();
    }
    let output = child.wait_with_output().expect("wait");
    assert!(!output.status.success(), "should error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rejected") || stderr.contains("expired") || stderr.contains("revoked"),
        "expected auth-error diagnostic; got: {stderr}"
    );

    // No credentials file should have been written.
    let creds_path = workspace.path().join("home/xdg/aristo/credentials");
    assert!(
        !creds_path.exists(),
        "credentials file should NOT exist after failed login"
    );
}

#[test]
fn auth_login_oauth_requires_repo_or_git_remote() {
    // No --repo flag, no .git/config — must refuse.
    let workspace = TempDir::new().unwrap();

    // Spawn a mock that won't be called (server arg present but no
    // request expected before we error out on repo derivation).
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let mut cmd = isolated(workspace.path());
    cmd.args(["auth", "login", "--server", &base])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().expect("run aristo");
    assert!(!output.status.success(), "must refuse without --repo");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--repo") || stderr.contains(".git/config"),
        "expected --repo hint; got: {stderr}"
    );
}

#[test]
fn auth_login_with_token_bypass_still_works() {
    // The OAuth refactor must not break the existing --token bypass.
    let workspace = TempDir::new().unwrap();

    let output = isolated(workspace.path())
        .args(["auth", "login", "--token", "arta_bypass_works"])
        .output()
        .expect("run aristo");
    assert!(
        output.status.success(),
        "bypass mode failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let creds_path = workspace.path().join("home/xdg/aristo/credentials");
    let body = std::fs::read_to_string(&creds_path).unwrap();
    assert!(
        body.contains("arta_bypass_works"),
        "credentials should contain the bypass token; got:\n{body}"
    );
}
