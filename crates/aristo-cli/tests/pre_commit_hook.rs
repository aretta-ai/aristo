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
//! `#[ignore]`'d until `aristo init` lands the hook installer (slice ≥8 in the
//! post-compaction plan). The test is in source so:
//! 1. it stays compile-checked alongside the real test suite
//! 2. the implementing slice removes `#[ignore]` in the same commit, per the
//!    `_pending/` → `active/` promotion convention applied to the imperative
//!    side
//! 3. the test contract is captured now, before the implementation, so the
//!    implementer treats it as the spec rather than a post-hoc audit

use assert_cmd::Command;

#[test]
#[ignore = "pending: requires `aristo init` hook installer + `aristo stamp`/`aristo lint` to be implemented"]
fn pre_commit_hook_runs_stamp_and_lint() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path();

    // 1. fresh git repo
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(repo)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "user.email", "test@aretta.dev"])
        .current_dir(repo)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(repo)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(repo)
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
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .assert()
        .success();
    let commit = Command::new("git")
        .args(["commit", "-m", "feat: stable_hash"])
        .current_dir(repo)
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
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .assert()
        .success();
    let blocked = Command::new("git")
        .args(["commit", "-m", "feat: empty_text"])
        .current_dir(repo)
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&blocked.get_output().stderr).to_string();
    assert!(
        stderr.contains("empty_text") || stderr.contains("lint"),
        "expected pre-commit hook's `aristo lint --check` to abort the commit; got stderr:\n{stderr}"
    );
}
