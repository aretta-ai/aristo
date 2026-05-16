//! Imperative integration test: `aristo init` installs a pre-commit hook that
//! runs `aristo stamp` (always) + `aristo lint --check` (per `[lint] pre_commit`
//! default — J6) against the staged content of a fresh `git init` repo.
//!
//! Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "2 · Daily authoring
//! loop", `L → l2` ("git commit triggers pre-commit hook → aristo stamp + lint").
//!
//! Why imperative (not trycmd): the test must drive a real `git init` + add +
//! commit cycle in a temp directory, observe the hook firing, and assert that
//! `.aristo/index.toml` is updated and lint findings cause the commit to abort.
//! That sequence isn't a single CLI invocation, so it doesn't fit a `console`-
//! fenced trycmd file.
//!
//! Un-ignored in slice 21 (the pre-commit hook implementation) per
//! CLAUDE.md §12A — promoted from `#[ignore]` to active in the same commit
//! that lands the real hook content. Counts as the imperative-side
//! equivalent of moving a `_pending/` `.md` scenario to `active/`.

use assert_cmd::Command;
use std::path::Path;

#[test]
fn pre_commit_hook_runs_stamp_and_lint() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path();

    // The hook script runs `aristo ...` and needs to find it on PATH. In
    // tests, the cargo-built binary lives next to its dependencies in
    // target/debug/. Prepend that directory to PATH for every git
    // invocation so the hook subprocess inherits it.
    let aristo_bin = assert_cmd::cargo::cargo_bin("aristo");
    let aristo_dir = aristo_bin.parent().expect("cargo bin has a parent dir");
    let new_path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut paths = vec![aristo_dir.to_path_buf()];
            paths.extend(std::env::split_paths(&existing));
            std::env::join_paths(paths).expect("join PATH entries")
        }
        None => aristo_dir.as_os_str().to_owned(),
    };
    let git = |dir: &Path| {
        let mut cmd = Command::new("git");
        cmd.current_dir(dir).env("PATH", &new_path);
        cmd
    };

    // 1. fresh git repo
    git(repo).args(["init", "--quiet"]).assert().success();
    git(repo)
        .args(["config", "user.email", "test@aretta.dev"])
        .assert()
        .success();
    git(repo)
        .args(["config", "user.name", "Test"])
        .assert()
        .success();
    git(repo)
        .args(["config", "commit.gpgsign", "false"])
        .assert()
        .success();

    // 2. minimal Cargo project so `aristo init` recognizes it
    std::fs::write(
        repo.join("Cargo.toml"),
        r#"[package]
name = "hook-test"
version = "0.0.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::create_dir(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "").unwrap();

    // 3. aristo init — installs hook, writes aristo.toml + .aristo/
    Command::cargo_bin("aristo")
        .unwrap()
        .arg("init")
        .current_dir(repo)
        .assert()
        .success();
    assert!(
        repo.join(".git/hooks/pre-commit").exists(),
        "expected aristo init to install .git/hooks/pre-commit"
    );

    // 4. add a well-formed annotation and commit — hook runs stamp; commit succeeds
    std::fs::write(
        repo.join("src/lib.rs"),
        r#"use aristo::intent;

#[intent("the function returns a stable hash of its input")]
pub fn stable_hash(_x: &[u8]) -> u64 { 0 }
"#,
    )
    .unwrap();
    git(repo).args(["add", "."]).assert().success();
    let commit = git(repo)
        .args(["commit", "-m", "feat: stable_hash"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&commit.get_output().stderr).to_string();
    assert!(
        stderr.contains("aristo stamp") || stderr.contains("annotations stamped"),
        "expected pre-commit hook to invoke `aristo stamp`; got stderr:\n{stderr}"
    );
    assert!(
        repo.join(".aristo/index.toml").exists(),
        "expected `aristo stamp` to populate .aristo/index.toml during commit"
    );

    // 5. add an empty-text annotation (lint violation) — hook's `aristo lint --check`
    //    should fail the commit with a non-zero exit
    std::fs::write(
        repo.join("src/lib.rs"),
        r#"use aristo::intent;

#[intent("the function returns a stable hash of its input")]
pub fn stable_hash(_x: &[u8]) -> u64 { 0 }

#[intent("")]
pub fn empty_text() {}
"#,
    )
    .unwrap();
    git(repo).args(["add", "."]).assert().success();
    let blocked = git(repo)
        .args(["commit", "-m", "feat: empty_text"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&blocked.get_output().stderr).to_string();
    assert!(
        stderr.contains("empty_text") || stderr.contains("lint"),
        "expected pre-commit hook's `aristo lint --check` to abort the commit; got stderr:\n{stderr}"
    );
}
