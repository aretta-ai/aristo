//! `aristo status` — imperative integration tests for the project-level
//! summary and the J5 stale-index preflight advisory.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn init_with_three_annotations(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        r#"
            #[aristo::intent("alpha", verify = "test", id = "alpha")] fn a() {}
            #[aristo::intent("bravo", verify = "neural", id = "bravo")] fn b() {}
            #[aristo::assume("external", id = "charlie")] fn c() {}
        "#,
    )
    .unwrap();
    aristo_in(dir).arg("stamp").assert().success();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("status")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn empty_workspace_shows_zero_annotations() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    aristo_in(tmp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(contains("Aristo SDK"))
        .stdout(contains("Total:"))
        .stdout(contains("0"));
}

#[test]
fn populated_workspace_breaks_down_counts() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    let assert = aristo_in(tmp.path()).arg("status").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Total:"), "got: {stdout}");
    assert!(stdout.contains("3"), "expected total=3 somewhere: {stdout}");
    assert!(stdout.contains("intent"), "got: {stdout}");
    assert!(stdout.contains("assume"), "got: {stdout}");
    assert!(stdout.contains("By verify level"), "got: {stdout}");
    assert!(stdout.contains("By status"), "got: {stdout}");
    assert!(stdout.contains("schema_version"), "got: {stdout}");
}

#[test]
fn status_emits_stale_preflight_when_source_newer_than_index() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    // Touch the source file to bump its mtime past the index's.
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("alpha", verify = "test", id = "alpha")] fn a() {}"#,
    )
    .unwrap();

    aristo_in(tmp.path())
        .arg("status")
        .assert()
        .success()
        .stderr(contains("may be stale relative to source"))
        .stderr(contains("Run `aristo stamp`"));
}

#[test]
fn status_no_warning_when_index_is_fresh() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    let assert = aristo_in(tmp.path()).arg("status").assert().success();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("may be stale"),
        "fresh index should not warn; got stderr: {stderr}"
    );
}

#[test]
fn status_surfaces_active_review_session_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "test-review", "--subject", "fixture"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(contains("Active review session"))
        .stdout(contains("kind=test-review"))
        .stdout(contains("subject=fixture"));
}

#[test]
fn status_surfaces_backlog_when_deferred_items_exist() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "test-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "decide", "--item", "a#0", "--bucket", "pending"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "decide", "--item", "b#1", "--bucket", "pending"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "exit"])
        .assert()
        .success();

    aristo_in(tmp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(contains("Review backlog"))
        .stdout(contains("test-review: 2 items"));
}

#[test]
fn status_omits_review_sections_when_no_session_or_backlog() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let assert = aristo_in(tmp.path()).arg("status").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains("Review backlog"),
        "no backlog should be reported when empty: {stdout}"
    );
    assert!(
        !stdout.contains("Active review session"),
        "no active session should be reported when none open: {stdout}"
    );
}
