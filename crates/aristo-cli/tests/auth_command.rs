//! End-to-end integration tests for `aristo auth {login, status,
//! logout}`. Spawns the actual `aristo` binary as a subprocess with
//! `HOME` / `XDG_CONFIG_HOME` / `ARETTA_TOKEN` set to test-controlled
//! values, so the user's real credentials are never touched.
//!
//! These tests use `Command::env_clear` first, then re-add the
//! minimum set of vars the binary needs (`PATH`, locale, etc.).
//! Without `env_clear`, parallel test runs could see an
//! `ARETTA_TOKEN` set by a flaky shell session.

use std::process::Command;

use tempfile::TempDir;

/// Path to the freshly-built `aristo` binary; cargo sets this env
/// var for integration tests under `tests/`.
fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Build an isolated `Command` that won't touch the user's real
/// `~/.config/aristo/credentials`. Inherits `PATH` so the binary
/// can find dynamic libs on macOS, but explicitly clears
/// `ARETTA_TOKEN` and pins `HOME` + `XDG_CONFIG_HOME` to `home`.
fn isolated(home: &std::path::Path) -> Command {
    let mut c = Command::new(aristo_bin());
    c.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        c.env("PATH", path);
    }
    // macOS dyld needs this so the test binary loads the right libs.
    #[cfg(target_os = "macos")]
    {
        if let Ok(p) = std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
            c.env("DYLD_FALLBACK_LIBRARY_PATH", p);
        }
    }
    c.env("HOME", home);
    c.env("XDG_CONFIG_HOME", home.join("xdg"));
    c
}

fn creds_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join("xdg/aristo/credentials")
}

// ─── auth status ──────────────────────────────────────────────────────────

#[test]
fn status_when_not_authenticated() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .expect("run aristo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("not authenticated"), "stdout: {stdout}");
    assert!(stdout.contains("aristo auth login"), "stdout: {stdout}");
    assert!(stdout.contains("ARETTA_TOKEN"), "stdout: {stdout}");
}

#[test]
fn status_reads_env_var_when_set() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .env("ARETTA_TOKEN", "env-test-tok")
        .output()
        .expect("run aristo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("authenticated"), "stdout: {stdout}");
    assert!(stdout.contains("ARETTA_TOKEN"), "stdout: {stdout}");
    // Must NOT print the token itself.
    assert!(
        !stdout.contains("env-test-tok"),
        "status MUST NOT print the token; stdout: {stdout}"
    );
}

#[test]
fn status_reads_credentials_file() {
    let tmp = TempDir::new().unwrap();
    // Drop a credentials file by hand (we test the login command's
    // file-creation path separately below).
    let p = creds_path(tmp.path());
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        r#"
[aretta]
token = "file-tok"
issued_at = "2026-05-20T00:00:00Z"
"#,
    )
    .unwrap();

    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("authenticated"), "stdout: {stdout}");
    // Path appears in the success message (cross-platform check —
    // just look for the filename, not the full prefix).
    assert!(stdout.contains("credentials"), "stdout: {stdout}");
    assert!(
        !stdout.contains("file-tok"),
        "status must not print token: {stdout}"
    );
}

#[test]
fn status_malformed_credentials_surfaces_error() {
    let tmp = TempDir::new().unwrap();
    let p = creds_path(tmp.path());
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "this is not TOML at all = = =").unwrap();

    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected non-zero exit on malformed creds"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("malformed"), "stderr: {stderr}");
    assert!(
        stderr.contains("aristo auth logout"),
        "stderr should hint recovery: {stderr}"
    );
}

// ─── auth login ───────────────────────────────────────────────────────────

#[test]
fn login_with_token_flag_persists_credentials_file() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "login", "--token", "flag-tok-12345"])
        .output()
        .expect("run aristo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("authenticated"), "stdout: {stdout}");
    // Must NOT echo the token.
    assert!(
        !stdout.contains("flag-tok-12345"),
        "login MUST NOT echo the token; stdout: {stdout}"
    );

    // Credentials file landed on disk under XDG path.
    let p = creds_path(tmp.path());
    assert!(p.exists(), "expected credentials at {p:?}");
    let body = std::fs::read_to_string(&p).unwrap();
    assert!(
        body.contains("flag-tok-12345"),
        "creds file should contain token"
    );
    assert!(
        body.contains("issued_at"),
        "creds file should include timestamp"
    );
}

#[test]
#[cfg(unix)]
fn login_sets_unix_0600_perms() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "login", "--token", "tok"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let p = creds_path(tmp.path());
    let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
}

#[test]
fn login_with_stdin_pipe() {
    let tmp = TempDir::new().unwrap();
    let mut child = isolated(tmp.path())
        .args(["auth", "login", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"piped-tok-67890\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let p = creds_path(tmp.path());
    let body = std::fs::read_to_string(&p).unwrap();
    assert!(body.contains("piped-tok-67890"));
}

#[test]
fn login_empty_token_rejected() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "login", "--token", "   "])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected non-zero exit on empty token"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no token"), "stderr: {stderr}");
    // No credentials file should have been created.
    assert!(!creds_path(tmp.path()).exists());
}

#[test]
fn login_then_status_round_trip() {
    let tmp = TempDir::new().unwrap();
    let _ = isolated(tmp.path())
        .args(["auth", "login", "--token", "round-trip-tok"])
        .output()
        .unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("authenticated"));
    assert!(
        !stdout.contains("round-trip-tok"),
        "status must not print token"
    );
}

// ─── auth logout ──────────────────────────────────────────────────────────

#[test]
fn logout_when_not_logged_in_is_noop_and_zero_exit() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "logout"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "logout when not logged in should be idempotent"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("not logged in") || stdout.contains("logged out"));
}

#[test]
fn logout_after_login_removes_file() {
    let tmp = TempDir::new().unwrap();
    let _ = isolated(tmp.path())
        .args(["auth", "login", "--token", "tok"])
        .output()
        .unwrap();
    assert!(creds_path(tmp.path()).exists());

    let out = isolated(tmp.path())
        .args(["auth", "logout"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("logged out"), "stdout: {stdout}");
    assert!(
        !creds_path(tmp.path()).exists(),
        "creds file should be gone"
    );
}

#[test]
fn logout_warns_when_env_var_still_set() {
    let tmp = TempDir::new().unwrap();
    let _ = isolated(tmp.path())
        .args(["auth", "login", "--token", "tok"])
        .output()
        .unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "logout"])
        .env("ARETTA_TOKEN", "still-set")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ARETTA_TOKEN"), "stdout: {stdout}");
    assert!(stdout.contains("still use it"), "stdout: {stdout}");
    // Token value must not appear.
    assert!(!stdout.contains("still-set"), "stdout: {stdout}");
}

// ─── login → status → logout → status full lifecycle ─────────────────────

#[test]
fn full_auth_lifecycle() {
    let tmp = TempDir::new().unwrap();

    // 1. status: not authenticated
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("not authenticated"));

    // 2. login
    let out = isolated(tmp.path())
        .args(["auth", "login", "--token", "lifecycle-tok"])
        .output()
        .unwrap();
    assert!(out.status.success());

    // 3. status: authenticated
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("authenticated"), "stdout: {stdout}");
    assert!(!stdout.contains("lifecycle-tok"));

    // 4. logout
    let out = isolated(tmp.path())
        .args(["auth", "logout"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("logged out"));

    // 5. status: not authenticated again
    let out = isolated(tmp.path())
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("not authenticated"));
}
