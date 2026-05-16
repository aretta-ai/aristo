//! `aristo list` — imperative integration tests.
//!
//! Covers: error paths (no workspace, no index), empty inventory, basic
//! enumeration with summary footer, J2 `--filter` (single + AND of
//! multiple), and `--json` round-trip.

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
            #[aristo::intent("first claim", verify = "test", id = "alpha")] fn a() {}
            #[aristo::intent("second claim", verify = "full", id = "bravo")] fn b() {}
            #[aristo::assume("external invariant")] fn c() {}
        "#,
    )
    .unwrap();
    aristo_in(dir).arg("stamp").assert().success();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("list")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn empty_workspace_lists_zero_with_message() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path()).arg("stamp").assert().success();

    aristo_in(tmp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(contains("0 annotations"));
}

#[test]
fn lists_all_annotations_sorted_with_summary_footer() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    let assert = aristo_in(tmp.path()).arg("list").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Sorted alphabetically by id.
    let alpha = stdout.find("alpha").unwrap();
    let bravo = stdout.find("bravo").unwrap();
    assert!(alpha < bravo, "expected alpha before bravo:\n{stdout}");

    assert!(stdout.contains("intent"));
    assert!(stdout.contains("assume"));
    assert!(stdout.contains("3 annotations"));
}

#[test]
fn filter_by_status_keeps_only_matches() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    // All three start as `unknown`. Filter on it — should see all three.
    aristo_in(tmp.path())
        .args(["list", "--filter", "status=unknown"])
        .assert()
        .success()
        .stdout(contains("3 matches"));

    // Filter on a status no entry has → zero matches.
    aristo_in(tmp.path())
        .args(["list", "--filter", "status=verified"])
        .assert()
        .success()
        .stdout(contains("0 matches"));
}

#[test]
fn multiple_filters_and_together() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    // status=unknown AND id=alpha → exactly one (alpha is unknown like all of them).
    let assert = aristo_in(tmp.path())
        .args(["list", "--filter", "status=unknown", "--filter", "id=alpha"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("alpha"));
    assert!(
        !stdout.contains("bravo"),
        "AND should exclude bravo: {stdout}"
    );
    assert!(stdout.contains("1 match"));
}

#[test]
fn filter_with_bad_grammar_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    aristo_in(tmp.path())
        .args(["list", "--filter", "kind=intent"]) // unknown key
        .assert()
        .failure()
        .stderr(contains("unknown filter key"));
}

#[test]
fn json_output_is_an_array_of_records() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    let assert = aristo_in(tmp.path())
        .args(["list", "--json"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("valid json array");
    assert_eq!(arr.len(), 3);
    let ids: Vec<String> = arr
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"alpha".to_string()));
    assert!(ids.contains(&"bravo".to_string()));
    // The assume's id is opaque (auto-generated `aret_<...>`); just confirm
    // there's a third record with kind=assume.
    let kinds: Vec<&str> = arr.iter().map(|v| v["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "assume").count(), 1);
}

#[test]
fn json_output_combines_with_filter() {
    let tmp = tempfile::tempdir().unwrap();
    init_with_three_annotations(tmp.path());

    let assert = aristo_in(tmp.path())
        .args(["list", "--filter", "id=alpha", "--json"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "alpha");
}
