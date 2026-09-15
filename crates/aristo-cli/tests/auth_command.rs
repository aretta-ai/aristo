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

/// Seed the store with one repo-scoped credential, the way a completed
/// `aristo auth login` leaves it. Tests never paste tokens: OAuth is the
/// only login, and CI reads `ARETTA_TOKEN` instead of the store.
fn seed_store(home: &std::path::Path, repo: &str, token: &str) {
    let p = creds_path(home);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        format!(
            "version = 2\n\n[[entries]]\nserver = \"https://code.aretta.ai\"\nrepo = \"{repo}\"\n\
             token = \"{token}\"\nminted_at = \"2026-09-15T00:00:00Z\"\n"
        ),
    )
    .unwrap();
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
fn login_then_status_round_trip() {
    let tmp = TempDir::new().unwrap();
    seed_store(tmp.path(), "owner/repo", "round-trip-tok");
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

#[test]
fn login_has_no_raw_token_mode() {
    // OAuth is the only login. The old paste flags are gone, not hidden.
    for flag in ["--token", "--stdin"] {
        let out = isolated(TempDir::new().unwrap().path())
            .args(["auth", "login", flag, "x"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "{flag} must be rejected");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("unexpected argument"), "{flag}: {stderr}");
    }
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
    seed_store(tmp.path(), "owner/repo", "tok");
    assert!(creds_path(tmp.path()).exists());

    let out = isolated(tmp.path())
        .args(["auth", "logout", "--repo", "owner/repo"])
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
    seed_store(tmp.path(), "owner/repo", "tok");
    let out = isolated(tmp.path())
        .args(["auth", "logout", "--repo", "owner/repo"])
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
    seed_store(tmp.path(), "owner/repo", "lifecycle-tok");

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
        .args(["auth", "logout", "--repo", "owner/repo"])
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

// ─── auth token ────────────────────────────────────────────────────────────

#[test]
fn token_prints_value_from_env_var() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "token"])
        .env("ARETTA_TOKEN", "arta_env_tok_123")
        .output()
        .expect("run aristo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Unlike `status`, `token` DOES print the value — and only the value.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "arta_env_tok_123", "stdout: {stdout}");
}

#[test]
fn token_prints_value_from_credentials_file() {
    let tmp = TempDir::new().unwrap();
    let p = creds_path(tmp.path());
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "[aretta]\ntoken = \"arta_file_tok_456\"\nissued_at = \"2026-05-20T00:00:00Z\"\nrepo = \"owner/repo\"\n",
    )
    .unwrap();

    let out = isolated(tmp.path())
        .args(["auth", "token", "--repo", "owner/repo"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "arta_file_tok_456", "stdout: {stdout}");
}

#[test]
fn token_errors_when_not_authenticated() {
    let tmp = TempDir::new().unwrap();
    let out = isolated(tmp.path())
        .args(["auth", "token"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected non-zero exit when no token is available"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not authenticated"), "stderr: {stderr}");
    assert!(stderr.contains("aristo auth login"), "stderr: {stderr}");
}
