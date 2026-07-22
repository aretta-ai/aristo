//! End-to-end tests for the multi-repo credential store surface:
//! several repos logged in at once, `auth token` resolving the right
//! one (by `--repo` and by cwd), `auth status` listing all, and
//! repo-scoped vs `--all` logout.
//!
//! Offline and hermetic — `--token` bypass logins, no network, no
//! dependency on the discovery endpoint.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Run `aristo <args>` with a shared isolated HOME/XDG (so the store
/// persists across calls in one test) and an explicit working directory
/// (so cwd-repo derivation is controlled, not inherited from the test
/// runner's own git repo).
fn run(home: &Path, cwd: &Path, args: &[&str]) -> Output {
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
    c.current_dir(cwd);
    c.args(args);
    c.output().expect("run aristo")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A directory whose `.git/config` names a GitHub `origin` remote, so
/// `aristo` derives `owner/repo` from the cwd.
fn git_workspace(parent: &Path, name: &str, owner_repo: &str) -> std::path::PathBuf {
    let ws = parent.join(name);
    let git = ws.join(".git");
    std::fs::create_dir_all(&git).unwrap();
    std::fs::write(
        git.join("config"),
        format!("[remote \"origin\"]\n    url = https://github.com/{owner_repo}\n"),
    )
    .unwrap();
    ws
}

#[test]
fn two_repos_by_flag_resolve_independently() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    // A non-git cwd so only the explicit --repo keys the entries.
    let cwd = tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();

    // Log in two repos on two servers via the bypass.
    let a = run(
        &home,
        &cwd,
        &[
            "auth",
            "login",
            "--token",
            "tok_A",
            "--repo",
            "org/repoA",
            "--server",
            "https://a.example.com",
        ],
    );
    assert!(a.status.success(), "login A: {}", stdout(&a));
    let b = run(
        &home,
        &cwd,
        &[
            "auth",
            "login",
            "--token",
            "tok_B",
            "--repo",
            "org/repoB",
            "--server",
            "https://b.example.com",
        ],
    );
    assert!(b.status.success(), "login B: {}", stdout(&b));

    // `auth token --repo` resolves each independently.
    let ta = run(&home, &cwd, &["auth", "token", "--repo", "org/repoA"]);
    assert_eq!(stdout(&ta).trim(), "tok_A");
    let tb = run(&home, &cwd, &["auth", "token", "--repo", "org/repoB"]);
    assert_eq!(stdout(&tb).trim(), "tok_B");

    // `auth status` lists both, servers + repos, and NO token values.
    let st = run(&home, &cwd, &["auth", "status"]);
    let s = stdout(&st);
    assert!(s.contains("org/repoA"), "status: {s}");
    assert!(s.contains("org/repoB"), "status: {s}");
    assert!(s.contains("a.example.com"), "status: {s}");
    assert!(s.contains("b.example.com"), "status: {s}");
    assert!(
        !s.contains("tok_A") && !s.contains("tok_B"),
        "status leaked a token: {s}"
    );
}

#[test]
fn auth_token_resolves_by_cwd_repo() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let ws_a = git_workspace(tmp.path(), "a", "org/repoA");
    let ws_b = git_workspace(tmp.path(), "b", "org/repoB");

    // Log in from each workspace with no --repo — the repo is derived
    // from the cwd's git remote.
    assert!(run(&home, &ws_a, &["auth", "login", "--token", "tok_A"])
        .status
        .success());
    assert!(run(&home, &ws_b, &["auth", "login", "--token", "tok_B"])
        .status
        .success());

    // `auth token` (no --repo) prints the credential for the cwd's repo.
    assert_eq!(
        stdout(&run(&home, &ws_a, &["auth", "token"])).trim(),
        "tok_A"
    );
    assert_eq!(
        stdout(&run(&home, &ws_b, &["auth", "token"])).trim(),
        "tok_B"
    );
}

#[test]
fn logout_repo_scoped_removes_only_that_entry() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cwd = tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();

    run(
        &home,
        &cwd,
        &["auth", "login", "--token", "tok_A", "--repo", "org/repoA"],
    );
    run(
        &home,
        &cwd,
        &["auth", "login", "--token", "tok_B", "--repo", "org/repoB"],
    );

    // Log out of just repoA.
    let out = run(&home, &cwd, &["auth", "logout", "--repo", "org/repoA"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("logged out"), "{}", stdout(&out));

    // repoB survives, repoA is gone.
    let st = stdout(&run(&home, &cwd, &["auth", "status"]));
    assert!(st.contains("org/repoB"), "status: {st}");
    assert!(!st.contains("org/repoA"), "repoA should be gone: {st}");
    assert_eq!(
        stdout(&run(&home, &cwd, &["auth", "token", "--repo", "org/repoB"])).trim(),
        "tok_B"
    );
    // repoA no longer resolves.
    let miss = run(&home, &cwd, &["auth", "token", "--repo", "org/repoA"]);
    assert!(
        !miss.status.success(),
        "repoA token should error after logout"
    );
}

#[test]
fn logout_all_clears_every_entry() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cwd = tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();

    run(
        &home,
        &cwd,
        &["auth", "login", "--token", "tok_A", "--repo", "org/repoA"],
    );
    run(
        &home,
        &cwd,
        &["auth", "login", "--token", "tok_B", "--repo", "org/repoB"],
    );

    let out = run(&home, &cwd, &["auth", "logout", "--all"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("logged out"), "{}", stdout(&out));

    let st = stdout(&run(&home, &cwd, &["auth", "status"]));
    assert!(st.contains("not authenticated"), "status: {st}");
}
