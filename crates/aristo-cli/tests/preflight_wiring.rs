//! Cross-command coverage for the J5 stale-index preflight.
//!
//! Per slice 19, the preflight wires into `aristo show`, `aristo list`,
//! and `aristo status`. Future slices wire it into `aristo verify`,
//! `aristo doc`, `aristo graph`, `aristo badge`, `aristo review` as they
//! ship. This file is the single regression site that proves "if a
//! command READS the index, it preflights" — adding a new reader
//! command means adding a case here.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn make_stale_workspace(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        r#"#[aristo::intent("alpha", verify = "test", id = "alpha")] fn a() {}"#,
    )
    .unwrap();
    aristo_in(dir).arg("stamp").assert().success();

    // Bump source mtime past the index's.
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(
        dir.join("src/lib.rs"),
        r#"#[aristo::intent("alpha v2", verify = "test", id = "alpha")] fn a() {}"#,
    )
    .unwrap();
}

#[test]
fn show_emits_stale_warning_when_source_newer_than_index() {
    let tmp = tempfile::tempdir().unwrap();
    make_stale_workspace(tmp.path());

    aristo_in(tmp.path())
        .args(["show", "alpha"])
        .assert()
        .success() // advisory only
        .stderr(contains("may be stale relative to source"));
}

#[test]
fn list_emits_stale_warning_when_source_newer_than_index() {
    let tmp = tempfile::tempdir().unwrap();
    make_stale_workspace(tmp.path());

    aristo_in(tmp.path())
        .arg("list")
        .assert()
        .success()
        .stderr(contains("may be stale relative to source"));
}

#[test]
fn stamp_does_not_emit_stale_warning_even_when_source_newer() {
    // Refresh commands are the refresh path; they regenerate the index,
    // so emitting "stale" before doing the refresh would be misleading.
    let tmp = tempfile::tempdir().unwrap();
    make_stale_workspace(tmp.path());

    let assert = aristo_in(tmp.path()).arg("stamp").assert().success();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("may be stale"),
        "stamp is the refresh path; should not preflight. got: {stderr}"
    );
}

#[test]
fn index_does_not_emit_stale_warning_even_when_source_newer() {
    let tmp = tempfile::tempdir().unwrap();
    make_stale_workspace(tmp.path());

    let assert = aristo_in(tmp.path()).arg("index").assert().success();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("may be stale"),
        "index is the refresh path; should not preflight. got: {stderr}"
    );
}

#[test]
fn list_warning_does_not_break_exit_code_or_stdout_content() {
    // Advisory only — the warning goes to stderr; stdout still has the
    // full listing and exit code is 0.
    let tmp = tempfile::tempdir().unwrap();
    make_stale_workspace(tmp.path());

    aristo_in(tmp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(contains("alpha")); // still lists the (stale) entry
}
