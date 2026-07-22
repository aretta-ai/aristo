//! Offline end-to-end tests for zero-config org discovery at
//! `aristo auth login`.
//!
//! All hermetic: local `TcpListener` mock servers stand in for the
//! discovery platform and the org conductor. `ARETTA_DISCOVERY_URL`
//! relocates the discovery platform (which defaults to prod) to a local
//! capture server, so no test ever touches the network. None of these
//! depend on the real conductor `/.well-known/aretta-org` endpoint being
//! built.
//!
//! Coverage:
//! - (a) discovery hit → login is redirected to the discovered org
//!   server and the announce says `(discovered for <repo>)`.
//! - (b) discovery 404 → login falls back to the platform default and
//!   proceeds there cleanly (no `discovered` suffix).
//! - (c) an explicit `--server` skips discovery entirely — the discovery
//!   probe records ZERO hits.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Clean-env `aristo` command with isolated HOME/XDG and no browser
/// pop-up. Crucially, `env_clear` means ARETTA_API_URL / ARETTA_TOKEN
/// are unset, so discovery is eligible unless the test opts out.
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
    c.env("ARISTO_NO_BROWSER", "1");
    c.current_dir(workspace);
    c
}

#[derive(Clone)]
struct Canned {
    status_line: &'static str,
    body: String,
}

impl Canned {
    fn ok(body: &str) -> Self {
        Canned {
            status_line: "200 OK",
            body: body.to_string(),
        }
    }
    fn not_found(body: &str) -> Self {
        Canned {
            status_line: "404 Not Found",
            body: body.to_string(),
        }
    }
}

#[derive(Debug)]
struct Rec {
    method: String,
    path: String,
    body: String,
}

/// A mock that serves `responses` in order — one per accepted
/// connection — recording each request. Joins after all are seen.
fn spawn_seq_mock(responses: Vec<Canned>) -> (String, thread::JoinHandle<Vec<Rec>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut recs = Vec::new();
        for resp in responses {
            let (mut stream, _) = listener.accept().expect("accept");
            recs.push(read_request(&mut stream));
            write_response(&mut stream, &resp);
        }
        recs
    });
    (base, handle)
}

/// A discovery probe that counts every connection it receives (and
/// answers with a poison result so a regression misroutes loudly rather
/// than passing silently). The returned counter is read after the CLI
/// exits: zero proves discovery never ran.
fn spawn_discovery_probe() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_c = Arc::clone(&hits);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            hits_c.fetch_add(1, Ordering::SeqCst);
            let _ = read_request(&mut stream);
            write_response(
                &mut stream,
                &Canned::ok(r#"{"org":"poison","base_url":"http://127.0.0.1:9"}"#),
            );
        }
    });
    (base, hits)
}

fn read_request(stream: &mut TcpStream) -> Rec {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).unwrap();
    }
    Rec {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn write_response(stream: &mut TcpStream, canned: &Canned) {
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

fn oauth_login_ok() -> Canned {
    Canned::ok(r#"{"url":"https://github.com/login/oauth/authorize?client_id=test"}"#)
}

fn cli_token_ok() -> Canned {
    Canned::ok(
        r#"{
            "arta_token": "arta_discovered_tok",
            "jwt": "jwt-blob",
            "user": { "id": 7, "login": "octocat" },
            "token_id": "tok_1",
            "repo_full_name": "owner/repo",
            "last_4": "_tok"
        }"#,
    )
}

/// Run `aristo auth login --repo owner/repo` with the given extra env,
/// piping an OAuth code into stdin. Returns (success, stdout, stderr).
fn run_login(workspace: &Path, extra_env: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = isolated(workspace);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.args(["auth", "login", "--repo", "owner/repo"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn aristo");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"oauth-code\n")
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn discovery_hit_redirects_login_to_the_org_server() {
    let workspace = tempfile::TempDir::new().unwrap();

    // The org conductor serves the two-step OAuth flow.
    let (org_base, org_handle) = spawn_seq_mock(vec![oauth_login_ok(), cli_token_ok()]);
    // The discovery platform maps the repo to that org.
    let (disco_base, disco_handle) = spawn_seq_mock(vec![Canned::ok(&format!(
        r#"{{"org":"acme","base_url":"{org_base}"}}"#
    ))]);

    let (ok, stdout, stderr) =
        run_login(workspace.path(), &[("ARETTA_DISCOVERY_URL", &disco_base)]);
    assert!(ok, "login failed: stdout={stdout} stderr={stderr}");

    // Discovery was queried once, for our repo.
    let disco = disco_handle.join().unwrap();
    assert_eq!(disco.len(), 1);
    assert_eq!(disco[0].method, "GET");
    assert!(
        disco[0].path.starts_with("/.well-known/aretta-org?repo="),
        "discovery path: {}",
        disco[0].path
    );

    // Login was redirected to the discovered org server (oauth_start +
    // exchange both landed there).
    let org = org_handle.join().unwrap();
    assert_eq!(org.len(), 2);
    assert_eq!(org[0].method, "GET");
    assert!(org[0].path.starts_with("/auth/login"), "{}", org[0].path);
    assert_eq!(org[1].method, "POST");
    assert_eq!(org[1].path, "/auth/cli-token");
    assert!(
        org[1].body.contains("oauth-code"),
        "exchange must forward the code to the discovered server: {}",
        org[1].body
    );

    // The announce names the discovered server + provenance.
    assert!(
        stderr.contains(&format!(
            "Authenticating against {org_base} (discovered for owner/repo)"
        )),
        "stderr: {stderr}"
    );

    // The credential persisted the discovered server.
    let creds =
        std::fs::read_to_string(workspace.path().join("home/xdg/aristo/credentials")).unwrap();
    assert!(creds.contains("arta_discovered_tok"), "creds: {creds}");
    assert!(
        creds.contains(&org_base),
        "creds should record discovered server: {creds}"
    );
}

#[test]
fn discovery_404_falls_back_to_platform_and_proceeds() {
    let workspace = tempfile::TempDir::new().unwrap();

    // One platform server: 404 for discovery, then serves OAuth itself —
    // proving login fell back to the platform default and proceeded
    // there cleanly.
    let (platform_base, handle) = spawn_seq_mock(vec![
        Canned::not_found(r#"{"error":"repo not mapped"}"#),
        oauth_login_ok(),
        cli_token_ok(),
    ]);

    let (ok, stdout, stderr) = run_login(
        workspace.path(),
        &[("ARETTA_DISCOVERY_URL", &platform_base)],
    );
    assert!(ok, "login failed: stdout={stdout} stderr={stderr}");

    let recs = handle.join().unwrap();
    assert_eq!(recs.len(), 3, "expected discovery + 2 oauth requests");
    assert!(
        recs[0].path.starts_with("/.well-known/aretta-org"),
        "first should be discovery: {}",
        recs[0].path
    );
    assert!(recs[1].path.starts_with("/auth/login"));
    assert_eq!(recs[2].path, "/auth/cli-token");

    // Fallback: announce names the platform, WITHOUT the discovered
    // provenance.
    assert!(
        stderr.contains(&format!("Authenticating against {platform_base}")),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("discovered for"),
        "must not claim discovery on a 404 miss; stderr: {stderr}"
    );
}

#[test]
fn explicit_server_skips_discovery_entirely() {
    let workspace = tempfile::TempDir::new().unwrap();

    // The --server target serves OAuth.
    let (org_base, org_handle) = spawn_seq_mock(vec![oauth_login_ok(), cli_token_ok()]);
    // A discovery probe that counts hits. If --server correctly skips
    // discovery, it is never touched.
    let (disco_base, hits) = spawn_discovery_probe();

    let mut cmd = isolated(workspace.path());
    cmd.env("ARETTA_DISCOVERY_URL", &disco_base)
        .args([
            "auth",
            "login",
            "--server",
            &org_base,
            "--repo",
            "owner/repo",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"oauth-code\n")
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "login failed: {stderr}");

    // The explicit server got the OAuth flow...
    let org = org_handle.join().unwrap();
    assert_eq!(org.len(), 2);

    // ...and discovery was never queried.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "an explicit --server must skip discovery; probe was hit"
    );

    // Announce names the flag source, not discovery.
    assert!(
        stderr.contains(&format!(
            "Authenticating against {org_base} (from --server)"
        )),
        "stderr: {stderr}"
    );
}
