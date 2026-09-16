//! End-to-end tests for the credential store surface: one entry per
//! server, `auth token` / `auth logout` selecting by `--server` or
//! `ARETTA_API_URL`, the single-entry rule, and `auth status`'s verdict.
//!
//! Offline and hermetic: the store is seeded directly, the way a
//! completed `aristo auth login` leaves it (OAuth is the only login;
//! its output is covered in `auth_oauth_login.rs`).

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Run `aristo <args>` with a shared isolated HOME/XDG, an explicit
/// working directory, and optional extra env.
fn run_env(home: &Path, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(aristo_bin());
    c.env_clear();
    if let Ok(p) = std::env::var("PATH") {
        c.env("PATH", p);
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
        c.env("DYLD_FALLBACK_LIBRARY_PATH", p);
    }
    c.env("HOME", home);
    c.env("XDG_CONFIG_HOME", home.join("xdg"));
    c.env("ARISTO_NO_BROWSER", "1");
    for (k, v) in env {
        c.env(k, v);
    }
    c.current_dir(cwd);
    c.args(args);
    c.output().expect("run aristo")
}

fn run(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    run_env(home, cwd, args, &[])
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Seed the store under `home` with `(server, token)` entries.
fn seed(home: &Path, entries: &[(&str, &str)]) {
    let p = home.join("xdg/aristo/credentials");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut body = String::from("version = 3\n");
    for (i, (server, token)) in entries.iter().enumerate() {
        body.push_str(&format!(
            "\n[[entries]]\nserver = \"{server}\"\ntoken = \"{token}\"\n\
             minted_at = \"2026-09-15T00:0{i}:00Z\"\nuser_login = \"alice\"\n"
        ));
    }
    std::fs::write(&p, body).unwrap();
}

fn sandbox() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    (tmp, home, work)
}

const ACME: &str = "https://acme.aretta.ai";
const OTHER: &str = "https://other.aretta.ai";

#[test]
fn single_entry_resolves_from_any_directory() {
    let (_tmp, home, work) = sandbox();
    seed(&home, &[(ACME, "tok_A")]);
    // No checkout, no selector: the one entry is the answer.
    assert_eq!(
        stdout(&run(&home, &work, &["auth", "token"])).trim(),
        "tok_A"
    );
    let st = run(&home, &work, &["auth", "status"]);
    assert!(st.status.success(), "{}", stdout(&st));
    let s = stdout(&st);
    assert!(
        s.contains("server: https://acme.aretta.ai   user: alice"),
        "{s}"
    );
    assert!(
        s.contains("commands use: server https://acme.aretta.ai."),
        "{s}"
    );
    assert!(!s.contains("tok_A"), "token leaked: {s}");
}

#[test]
fn several_servers_need_a_selector() {
    let (_tmp, home, work) = sandbox();
    seed(&home, &[(ACME, "tok_A"), (OTHER, "tok_B")]);

    // Unselected: status lists both and exits 1; token errors naming the fix.
    let st = run(&home, &work, &["auth", "status"]);
    assert_eq!(st.status.code(), Some(1));
    let s = stdout(&st);
    assert!(s.contains("2 server(s)"), "{s}");
    assert!(
        s.contains("several servers on file") && s.contains("--server <url>"),
        "{s}"
    );
    let miss = run(&home, &work, &["auth", "token"]);
    assert!(!miss.status.success());
    assert!(String::from_utf8_lossy(&miss.stderr).contains("--server <url>"));

    // --server selects.
    assert_eq!(
        stdout(&run(&home, &work, &["auth", "token", "--server", OTHER])).trim(),
        "tok_B"
    );
    // ARETTA_API_URL selects too, for status and token alike.
    let st = run_env(
        &home,
        &work,
        &["auth", "status"],
        &[("ARETTA_API_URL", ACME)],
    );
    assert!(st.status.success(), "{}", stdout(&st));
    assert!(
        stdout(&st)
            .contains("commands use: server https://acme.aretta.ai (named by ARETTA_API_URL)"),
        "{}",
        stdout(&st)
    );
    assert_eq!(
        stdout(&run_env(
            &home,
            &work,
            &["auth", "token"],
            &[("ARETTA_API_URL", ACME)]
        ))
        .trim(),
        "tok_A"
    );
}

#[test]
fn a_named_server_that_is_not_on_file_is_told_so() {
    let (_tmp, home, work) = sandbox();
    seed(&home, &[(ACME, "tok_A")]);
    let st = run_env(
        &home,
        &work,
        &["auth", "status"],
        &[("ARETTA_API_URL", OTHER)],
    );
    assert_eq!(st.status.code(), Some(1));
    let s = stdout(&st);
    assert!(
        s.contains("not on file")
            && s.contains("aristo auth login --server https://other.aretta.ai"),
        "{s}"
    );
    let miss = run(&home, &work, &["auth", "token", "--server", OTHER]);
    assert!(!miss.status.success());
    assert!(
        String::from_utf8_lossy(&miss.stderr).contains("no credential for https://other.aretta.ai")
    );
}

#[test]
fn logout_by_server_or_single_entry_or_all() {
    let (_tmp, home, work) = sandbox();
    seed(&home, &[(ACME, "tok_A"), (OTHER, "tok_B")]);

    // Several on file, none named → error, nothing changed.
    let out = run(&home, &work, &["auth", "logout"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stdout(&run(&home, &work, &["auth", "status"])).contains("2 server(s)"));

    // Named → only that one goes.
    let out = run(&home, &work, &["auth", "logout", "--server", ACME]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("logged out of https://acme.aretta.ai"));
    let s = stdout(&run(&home, &work, &["auth", "status"]));
    assert!(
        s.contains("other.aretta.ai") && !s.contains("acme.aretta.ai"),
        "{s}"
    );

    // Single entry → bare logout removes it and the file.
    let out = run(&home, &work, &["auth", "logout"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(!home.join("xdg/aristo/credentials").exists());

    // --all works on anything, idempotently.
    seed(&home, &[(ACME, "tok_A"), (OTHER, "tok_B")]);
    assert!(run(&home, &work, &["auth", "logout", "--all"])
        .status
        .success());
    assert!(stdout(&run(&home, &work, &["auth", "status"])).contains("not signed in"));
}

#[test]
fn a_v2_per_repo_file_still_reads_one_entry_per_server() {
    // A store written by 0.7.x: several entries for one server, one per
    // repo. The newest per server is kept; the repo is ignored.
    let (_tmp, home, work) = sandbox();
    let p = home.join("xdg/aristo/credentials");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        r#"version = 2

[[entries]]
server = "https://acme.aretta.ai"
repo = "acme/a"
token = "older"
minted_at = "2026-09-14T00:00:00Z"

[[entries]]
server = "https://acme.aretta.ai"
repo = "acme/b"
token = "newer"
minted_at = "2026-09-15T00:00:00Z"
"#,
    )
    .unwrap();
    assert_eq!(
        stdout(&run(&home, &work, &["auth", "token"])).trim(),
        "newer"
    );
    let s = stdout(&run(&home, &work, &["auth", "status"]));
    assert!(s.contains("1 server(s)"), "{s}");
}
