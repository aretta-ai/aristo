//! `aristo statusline` — ambient status segment integration tests
//! (Phase 18 #9, nudge surface v2).

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn write_lib(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), content).unwrap();
}

#[test]
fn statusline_shows_backlog_for_an_unverified_unreviewed_intent() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    // v2 Hybrid format: review = remaining count, verify = done/total coverage.
    // The fresh intent is unreviewed and not yet terminal-clean → "1 review",
    // "0/1 verified" (no cached tier in a non-session invocation).
    aristo_in(tmp.path())
        .arg("statusline")
        .assert()
        .success()
        .stdout(contains("aristo"))
        .stdout(contains("1 review"))
        .stdout(contains("0/1 verified"));
}

#[test]
fn statusline_is_silent_outside_a_workspace() {
    // Tolerant: no aristo workspace → empty segment, exit 0 (never disrupt
    // the status bar).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("statusline")
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn statusline_is_silent_when_aggressiveness_off() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let cfg = tmp.path().join("aristo.toml");
    let contents = fs::read_to_string(&cfg).unwrap();
    fs::write(
        &cfg,
        contents.replace("aggressiveness = \"medium\"", "aggressiveness = \"off\""),
    )
    .unwrap();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .arg("statusline")
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}
