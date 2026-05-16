//! `aristo index` — imperative integration tests for the surfaces trycmd
//! can't easily exercise:
//!
//! - the no-workspace error path (can't share a sandbox with the success
//!   path because the success path creates the workspace)
//! - actual index file content shape (TOML round-trip via aristo-core)
//! - permissive-mode skip-with-warning behavior on invalid annotations
//! - cycle-detection error reporting

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"))
        .stderr(contains("aristo init"));
}

#[test]
fn writes_meta_only_index_for_zero_annotations() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .success()
        .stdout(contains("ok: index regenerated (0 annotations)"));

    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).expect("index round-trips");
    assert_eq!(parsed.entries.len(), 0);
    assert!(parsed.meta.generated_by.unwrap().contains("aristo index"));
}

#[test]
fn indexes_intent_attribute_on_a_function() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"
            #[aristo::intent("returns 42", verify = "test", id = "returns_forty_two")]
            fn answer() -> i32 { 42 }
        "#,
    )
    .unwrap();

    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .success()
        .stdout(contains("Found 1 annotations"));

    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    let id = aristo_core::index::AnnotationId::parse("returns_forty_two").unwrap();
    let entry = parsed.entries.get(&id).expect("entry by readable id");
    match entry {
        aristo_core::index::IndexEntry::Intent(e) => {
            assert_eq!(e.text, "returns 42");
            assert_eq!(e.site, "fn answer (line 2)");
            assert!(e.file.ends_with("lib.rs"));
        }
        other => panic!("expected Intent, got {other:?}"),
    }
}

#[test]
fn assigns_opaque_id_when_user_omits_id() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("nameless")] fn x() {}"#,
    )
    .unwrap();

    aristo_in(tmp.path()).arg("index").assert().success();

    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    let (id, _) = parsed.entries.iter().next().unwrap();
    assert!(
        id.as_str().starts_with("aret_"),
        "expected aret_<random> id, got {id}"
    );
}

#[test]
fn permissive_mode_skips_invalid_annotations_with_warning() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"
            #[aristo::intent("good", verify = "test", id = "good_one")]
            fn good() {}

            #[aristo::intent("bad id", verify = "test", id = "FooBar")]
            fn bad() {}
        "#,
    )
    .unwrap();

    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .success() // exit 0 — invalid annotations are warnings, not errors
        .stderr(contains("warning: skipping"))
        .stderr(contains("FooBar"))
        .stdout(contains("ok: index regenerated (1 annotations)"));

    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).unwrap();
    assert_eq!(parsed.entries.len(), 1, "only the valid annotation indexed");
}

#[test]
fn errors_on_cycle_in_parent_graph() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"
            #[aristo::intent("a", verify = "test", id = "a", parent = "b")]
            fn a() {}

            #[aristo::intent("b", verify = "test", id = "b", parent = "a")]
            fn b() {}
        "#,
    )
    .unwrap();

    aristo_in(tmp.path())
        .arg("index")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("cycle"))
        .stderr(contains("No files modified"));

    // Index file must still be the empty one written at init time —
    // cycle errors don't corrupt prior state.
    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).unwrap();
    assert_eq!(parsed.entries.len(), 0);
}

#[test]
fn rerun_overwrites_atomically_no_partial_file() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("v1", verify = "test", id = "first")] fn v1() {}"#,
    )
    .unwrap();
    aristo_in(tmp.path()).arg("index").assert().success();

    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("v2", verify = "test", id = "first")] fn v2() {}"#,
    )
    .unwrap();
    aristo_in(tmp.path()).arg("index").assert().success();

    let index = fs::read_to_string(tmp.path().join(".aristo/index.toml")).unwrap();
    let parsed: aristo_core::index::IndexFile = toml::from_str(&index).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    let id = aristo_core::index::AnnotationId::parse("first").unwrap();
    if let aristo_core::index::IndexEntry::Intent(e) = parsed.entries.get(&id).unwrap() {
        assert_eq!(e.text, "v2", "second index call must overwrite, not append");
    }
}

#[test]
fn all_flag_is_no_op_in_slice_16() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#[aristo::intent("x", verify = "test", id = "first")] fn x() {}"#,
    )
    .unwrap();

    let with = aristo_in(tmp.path())
        .args(["index", "--all"])
        .output()
        .unwrap();
    let without = aristo_in(tmp.path()).arg("index").output().unwrap();
    assert!(with.status.success());
    assert!(without.status.success());
    // The two outputs should be identical apart from the timestamp in
    // generated_at (which is in the toml file, not stdout).
    let with_stdout = String::from_utf8(with.stdout)
        .unwrap()
        .replace(char::is_whitespace, "");
    let without_stdout = String::from_utf8(without.stdout)
        .unwrap()
        .replace(char::is_whitespace, "");
    assert_eq!(
        with_stdout, without_stdout,
        "--all must be a no-op in slice 16"
    );
}
