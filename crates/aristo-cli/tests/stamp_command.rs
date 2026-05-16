//! `aristo stamp` — imperative integration tests for drift detection,
//! `--check` CI mode, and the prior-status preservation contract.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn read_index(root: &Path) -> aristo_core::index::IndexFile {
    let text = fs::read_to_string(root.join(".aristo/index.toml")).unwrap();
    toml::from_str(&text).expect("index round-trips")
}

fn write_lib(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), content).unwrap();
}

fn lookup<'a>(
    idx: &'a aristo_core::index::IndexFile,
    id: &str,
) -> &'a aristo_core::index::IndexEntry {
    let parsed = aristo_core::index::AnnotationId::parse(id).unwrap();
    idx.entries
        .get(&parsed)
        .unwrap_or_else(|| panic!("no entry `{id}`"))
}

fn force_status(root: &Path, id: &str, status: aristo_core::index::Status) {
    // Read, mutate, write — emulates what `aristo verify` will do later
    // when verification flips an entry's status.
    let mut idx = read_index(root);
    let parsed = aristo_core::index::AnnotationId::parse(id).unwrap();
    let entry = idx.entries.get_mut(&parsed).unwrap();
    match entry {
        aristo_core::index::IndexEntry::Intent(e) => e.status = status,
        aristo_core::index::IndexEntry::Assume(e) => e.status = status,
    }
    let text = toml::to_string_pretty(&idx).unwrap();
    fs::write(root.join(".aristo/index.toml"), text).unwrap();
}

#[test]
fn stamp_on_fresh_workspace_writes_initial_index() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("hello", verify = "test", id = "greeting")] fn x() {}"#,
    );

    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("ok: stamped 1 annotations"))
        .stdout(contains("new: 1"));

    let idx = read_index(tmp.path());
    assert_eq!(idx.entries.len(), 1);
}

#[test]
fn stamp_preserves_status_when_body_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Simulate a prior `aristo verify` having set status to Tested.
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Re-stamp without source changes — Tested must be preserved.
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Tested,
            "body unchanged → status preserved"
        );
    }
}

#[test]
fn stamp_flips_verified_to_stale_on_body_change() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 1 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Edit body — same text, different body.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() -> i32 { 99 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("body-drifted: 1"))
        .stdout(contains("now Stale"));

    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(e.status, aristo_core::index::Status::Stale);
    }
}

#[test]
fn stamp_flips_text_changed_body_held_preserves_status() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v1", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    force_status(tmp.path(), "a", aristo_core::index::Status::Tested);

    // Edit ONLY text (re-word the intent prose); body unchanged.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("v2", verify = "test", id = "a")] fn x() -> i32 { 42 }"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("text-changed: 1"))
        .stdout(contains("review cache invalidated"));

    let idx = read_index(tmp.path());
    if let aristo_core::index::IndexEntry::Intent(e) = lookup(&idx, "a") {
        assert_eq!(
            e.status,
            aristo_core::index::Status::Tested,
            "text-only edit holds status — review cache, not verify, is the affected layer"
        );
        assert_eq!(e.text, "v2");
    }
}

#[test]
fn stamp_drops_removed_annotations_from_index() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("keep", verify = "test", id = "kept")] fn k() {}
            #[aristo::intent("drop", verify = "test", id = "dropped")] fn d() {}
        "#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Remove the second annotation by editing source.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("keep", verify = "test", id = "kept")] fn k() {}"#,
    );
    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .success()
        .stdout(contains("removed: 1"));

    let idx = read_index(tmp.path());
    assert_eq!(idx.entries.len(), 1);
    assert!(idx
        .entries
        .contains_key(&aristo_core::index::AnnotationId::parse("kept").unwrap()));
}

#[test]
fn check_mode_does_not_write_when_index_matches() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("x", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    let mtime_before = fs::metadata(tmp.path().join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .success()
        .stdout(contains("up to date"));

    let mtime_after = fs::metadata(tmp.path().join(".aristo/index.toml"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime_before, mtime_after, "--check must not write");
}

#[test]
fn check_mode_exits_nonzero_when_index_is_stale() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();

    // Now ADD a new annotation in source — index is stale relative to source.
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a")] fn x() {}
            #[aristo::intent("b", verify = "test", id = "b")] fn y() {}
        "#,
    );

    aristo_in(tmp.path())
        .args(["stamp", "--check"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("out of sync"));
}

#[test]
fn check_mode_does_not_corrupt_existing_index_on_diff() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("a", verify = "test", id = "a")] fn x() {}"#,
    );
    aristo_in(tmp.path()).arg("stamp").assert().success();
    let before = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();

    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a")] fn x() {}
            #[aristo::intent("b", verify = "test", id = "b")] fn y() {}
        "#,
    );
    let _ = aristo_in(tmp.path()).args(["stamp", "--check"]).output();

    let after = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    assert_eq!(
        before, after,
        "--check must leave the index file byte-identical"
    );
}

#[test]
fn cycle_in_source_aborts_stamp_with_diagnostic() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(
        tmp.path(),
        r#"
            #[aristo::intent("a", verify = "test", id = "a", parent = "b")] fn a() {}
            #[aristo::intent("b", verify = "test", id = "b", parent = "a")] fn b() {}
        "#,
    );

    aristo_in(tmp.path())
        .arg("stamp")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("cycle"))
        .stderr(contains("No files modified"));
}
