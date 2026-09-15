//! End-to-end tests for the multi-repo credential store surface:
//! several repos on file at once, `auth token` resolving the right one
//! (by `--repo` and by cwd), `auth status` listing all with the
//! per-directory verdict, and repo-scoped vs `--all` logout.
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

/// Seed the store under `home` with `(server, repo, token)` entries.
fn seed(home: &Path, entries: &[(&str, &str, &str)]) {
    let p = home.join("xdg/aristo/credentials");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut body = String::from("version = 2\n");
    for (i, (server, repo, token)) in entries.iter().enumerate() {
        body.push_str(&format!(
            "\n[[entries]]\nserver = \"{server}\"\nrepo = \"{repo}\"\ntoken = \"{token}\"\n\
             minted_at = \"2026-09-15T00:0{i}:00Z\"\n"
        ));
    }
    std::fs::write(&p, body).unwrap();
}

fn fresh_home(tmp: &TempDir) -> std::path::PathBuf {
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    home
}

fn plain_dir(tmp: &TempDir, name: &str) -> std::path::PathBuf {
    let d = tmp.path().join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn two_repos_by_flag_resolve_independently() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    // A non-git cwd so only the explicit --repo picks an entry.
    let cwd = plain_dir(&tmp, "work");
    seed(
        &home,
        &[
            ("https://a.example.com", "org/repoA", "tok_A"),
            ("https://b.example.com", "org/repoB", "tok_B"),
        ],
    );

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
    let home = fresh_home(&tmp);
    let ws_a = git_workspace(tmp.path(), "a", "org/repoA");
    let ws_b = git_workspace(tmp.path(), "b", "org/repoB");
    seed(
        &home,
        &[
            ("https://code.aretta.ai", "org/repoA", "tok_A"),
            ("https://code.aretta.ai", "org/repoB", "tok_B"),
        ],
    );

    // `auth token` (no --repo) prints the credential for the cwd's repo.
    assert_eq!(
        stdout(&run(&home, &ws_a, &["auth", "token"])).trim(),
        "tok_A"
    );
    assert_eq!(
        stdout(&run(&home, &ws_b, &["auth", "token"])).trim(),
        "tok_B"
    );
    // Outside a checkout nothing resolves without --repo.
    let plain = plain_dir(&tmp, "plain");
    let miss = run(&home, &plain, &["auth", "token"]);
    assert!(!miss.status.success());
    let err = String::from_utf8_lossy(&miss.stderr);
    assert!(err.contains("--repo"), "{err}");
}

#[test]
fn logout_repo_scoped_removes_only_that_entry() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    let cwd = plain_dir(&tmp, "work");
    seed(
        &home,
        &[
            ("https://code.aretta.ai", "org/repoA", "tok_A"),
            ("https://code.aretta.ai", "org/repoB", "tok_B"),
        ],
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
    let miss = run(&home, &cwd, &["auth", "token", "--repo", "org/repoA"]);
    assert!(
        !miss.status.success(),
        "repoA token should error after logout"
    );
}

#[test]
fn logout_outside_a_checkout_needs_repo_or_all() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    let cwd = plain_dir(&tmp, "work");
    seed(&home, &[("https://code.aretta.ai", "org/repoA", "tok_A")]);

    let out = run(&home, &cwd, &["auth", "logout"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--repo") && err.contains("--all"), "{err}");
    // Nothing was removed.
    assert!(stdout(&run(&home, &cwd, &["auth", "status"])).contains("org/repoA"));
}

#[test]
fn logout_all_clears_every_entry() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    let cwd = plain_dir(&tmp, "work");
    seed(
        &home,
        &[
            ("https://code.aretta.ai", "org/repoA", "tok_A"),
            ("https://code.aretta.ai", "org/repoB", "tok_B"),
        ],
    );

    let out = run(&home, &cwd, &["auth", "logout", "--all"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("logged out"), "{}", stdout(&out));

    let st = stdout(&run(&home, &cwd, &["auth", "status"]));
    assert!(st.contains("not authenticated"), "status: {st}");
}

// ─── status says which entry this checkout resolves to (#77) ──────────────

#[test]
fn status_verdict_follows_the_checkout() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    let ws_a = git_workspace(tmp.path(), "a", "org/repoA");
    let ws_fork = git_workspace(tmp.path(), "fork", "alice/repoB");
    seed(
        &home,
        &[
            ("https://code.aretta.ai", "org/repoA", "tok_A"),
            ("https://code.aretta.ai", "org/repoB", "tok_B"),
        ],
    );

    let st_a = stdout(&run(&home, &ws_a, &["auth", "status"]));
    assert!(
        st_a.contains(
            "this checkout (org/repoA) resolves to: server https://code.aretta.ai, repo org/repoA"
        ),
        "status: {st_a}"
    );
    let st = stdout(&run(&home, &ws_fork, &["auth", "status"]));
    assert!(
        st.contains("this checkout (alice/repoB) resolves to: no stored credential"),
        "status: {st}"
    );
    assert!(
        st.contains("aristo auth login --repo alice/repoB"),
        "status: {st}"
    );
}

#[test]
fn status_outside_a_github_checkout_says_so_and_resolves_nothing() {
    let tmp = TempDir::new().unwrap();
    let home = fresh_home(&tmp);
    let plain = plain_dir(&tmp, "plain");
    seed(&home, &[("https://code.aretta.ai", "org/repoA", "tok_A")]);

    let st = stdout(&run(&home, &plain, &["auth", "status"]));
    assert!(st.contains("not a GitHub checkout"), "status: {st}");
    assert!(st.contains("no .git/config"), "status: {st}");
    assert!(
        st.contains("resolves to: no stored credential (1 on file"),
        "no single-entry fallback: {st}"
    );
}
