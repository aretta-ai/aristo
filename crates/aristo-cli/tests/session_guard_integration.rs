//! Slice 27.5 step 3 — verifies that every mutating aristo command
//! refuses while a review session is active. Read-only commands and
//! the worker-facing queue peek/pop paths continue to function.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn workspace_with_active_session(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
    aristo_in(dir)
        .args([
            "session",
            "start",
            "critique-review",
            "--subject",
            "src/foo.rs",
        ])
        .assert()
        .success();
}

#[test]
fn stamp_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .failure()
        .stderr(contains("active review session blocks `aristo stamp`"))
        .stderr(contains("aristo session exit"));
}

#[test]
fn stamp_check_allowed_during_active_session() {
    // --check is read-only; must continue to work so CI gates can
    // inspect drift without being blocked by a developer's open review.
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .success();
}

#[test]
fn index_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .failure()
        .stderr(contains("active review session blocks `aristo index`"));
}

#[test]
fn verify_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .stderr(contains("active review session blocks `aristo verify`"));
}

#[test]
fn verify_apply_verdicts_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .failure()
        .stderr(contains("aristo verify --apply-verdicts"));
}

#[test]
fn verify_pop_next_allowed_during_active_session() {
    // Workers must keep functioning so an open session of any kind
    // doesn't strand in-flight verify dispatch.
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["verify", "--pop-next"])
        .assert()
        .success();
}

#[test]
fn verify_queue_status_allowed_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["verify", "--queue-status"])
        .assert()
        .success();
}

#[test]
fn critique_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["critique", "--filter", "id=foo"])
        .assert()
        .failure()
        .stderr(contains("active review session blocks `aristo critique`"));
}

#[test]
fn critique_apply_findings_refuses_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["critique", "--apply-findings"])
        .assert()
        .failure()
        .stderr(contains("aristo critique --apply-findings"));
}

#[test]
fn read_only_commands_allowed_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    // `list` reads index; should not be blocked even with an active
    // session. (We don't assert success on commands that need a Cargo
    // workspace like `lang` — the temp dir has no Cargo.toml.)
    aristo_in(tmp.path()).args(["list"]).assert().success();
    aristo_in(tmp.path()).args(["status"]).assert().success();
}

#[test]
fn session_commands_allowed_during_active_session() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "status"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "decide", "--item", "x#0", "--bucket", "accepted"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "exit"])
        .assert()
        .success();
}

#[test]
fn stamp_works_again_after_session_exit() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_active_session(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "abort", "--yes"])
        .assert()
        .success();
    aristo_in(tmp.path()).arg("stamp").assert().success();
}
