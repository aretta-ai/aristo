//! `aristo show` — imperative integration tests.
//!
//! Covers the surfaces trycmd can't easily express:
//! - error paths (no workspace, missing index, no match, did-you-mean)
//! - structured-output round-trip (JSON parses back into the same record)
//! - selector dispatch (id vs `fn name` vs `file:line`)

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn init_with_one_intent(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        r#"#[aristo::intent("returns the answer", verify = "test", id = "returns_forty_two")] fn answer() -> i32 { 42 }"#,
    )
    .unwrap();
    aristo_in(dir).arg("stamp").assert().success();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["show", "any_id"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn errors_when_index_missing() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::remove_file(tmp.path().join(".aristo/index.toml")).unwrap();

    aristo_in(tmp.path())
        .args(["show", "x"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("no .aristo/index.toml"))
        .stderr(contains("aristo stamp"));
}

#[test]
fn show_by_id_prints_full_record_with_text_block() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    aristo_in(tmp.path())
        .args(["show", "returns_forty_two"])
        .assert()
        .success()
        .stdout(contains("returns_forty_two (intent)"))
        .stdout(contains("status:"))
        .stdout(contains("verify:"))
        .stdout(contains("file:"))
        .stdout(contains("site:"))
        .stdout(contains("text_hash:"))
        .stdout(contains("body_hash:"))
        .stdout(contains("Text:"))
        .stdout(contains("returns the answer"));
}

#[test]
fn show_by_unknown_id_offers_did_you_mean_with_close_match() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    aristo_in(tmp.path())
        .args(["show", "returns_fortytwo"]) // missing underscore
        .assert()
        .failure()
        .code(1)
        .stderr(contains("no annotation with id `returns_fortytwo`"))
        .stderr(contains("Did you mean"))
        .stderr(contains("returns_forty_two"));
}

#[test]
fn show_by_unknown_id_with_no_close_match_lists_no_suggestion() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    let assert = aristo_in(tmp.path())
        .args(["show", "completely_different_thing_zzz"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains(
            "no annotation with id `completely_different_thing_zzz`",
        ));
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("Did you mean"),
        "unrelated id shouldn't trigger did-you-mean; got: {stderr}"
    );
}

#[test]
fn show_lists_children_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"
            #[aristo::intent("parent claim", verify = "test", id = "parent_a")] fn p() {}
            #[aristo::intent("child of A", verify = "test", id = "child_a", parent = "parent_a")] fn c1() {}
            #[aristo::intent("another child of A", verify = "test", id = "child_b", parent = "parent_a")] fn c2() {}
        "#,
    )
    .unwrap();
    aristo_in(tmp.path()).arg("stamp").assert().success();

    aristo_in(tmp.path())
        .args(["show", "parent_a"])
        .assert()
        .success()
        .stdout(contains("Children"))
        .stdout(contains("child_a"))
        .stdout(contains("child_b"));
}

#[test]
fn show_json_output_round_trips_through_serde() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    let assert = aristo_in(tmp.path())
        .args(["show", "returns_forty_two", "--json"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("--json output is valid json");
    assert_eq!(v["id"], "returns_forty_two");
    assert_eq!(v["kind"], "intent");
    assert_eq!(v["status"], "unknown");
    assert_eq!(v["verify"], "test");
}

#[test]
fn show_by_function_name_single_match_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    aristo_in(tmp.path())
        .args(["show", "fn answer"])
        .assert()
        .success()
        .stdout(contains("returns_forty_two"));
}

#[test]
fn show_by_function_name_no_match_emits_hint() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_one_intent(tmp.path());

    aristo_in(tmp.path())
        .args(["show", "fn does_not_exist"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("No items matching `fn does_not_exist`"));
}

#[test]
fn show_by_function_name_multi_match_lists_disambiguation() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    // Two `fn seek` in different modules, both annotated.
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"
            mod a {
                #[aristo::intent("a-seek invariant", verify = "test", id = "seek_a")]
                pub fn seek() {}
            }
            mod b {
                #[aristo::intent("b-seek invariant", verify = "test", id = "seek_b")]
                pub fn seek() {}
            }
        "#,
    )
    .unwrap();
    aristo_in(tmp.path()).arg("stamp").assert().success();

    aristo_in(tmp.path())
        .args(["show", "fn seek"])
        .assert()
        .success()
        .stdout(contains("Found 2 sites"))
        .stdout(contains("seek_a"))
        .stdout(contains("seek_b"));
}
