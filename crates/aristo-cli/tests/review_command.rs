//! `aristo review` — authored-intent review surface integration tests
//! (Phase 18 #7).

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

/// init → one intent → stamp → review.
fn repo_with_one_intent() -> tempfile::TempDir {
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
    tmp
}

#[test]
fn review_lists_an_unreviewed_authored_intent() {
    let tmp = repo_with_one_intent();
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 1"))
        .stdout(contains("\"id\": \"a\""))
        .stdout(contains("does a thing"));
}

#[test]
fn review_human_readout_prompts_to_mark() {
    let tmp = repo_with_one_intent();
    aristo_in(tmp.path())
        .arg("review")
        .assert()
        .success()
        .stdout(contains("await review"))
        .stdout(contains("aristo review --mark"));
}

#[test]
fn marking_an_intent_clears_it_from_the_backlog() {
    let tmp = repo_with_one_intent();
    aristo_in(tmp.path())
        .args(["review", "--mark", "a"])
        .assert()
        .success()
        .stdout(contains("Marked 1 intent(s) reviewed"));

    // Now the backlog is empty.
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 0"));

    aristo_in(tmp.path())
        .arg("review")
        .assert()
        .success()
        .stdout(contains("All authored intents are reviewed"));
}

#[test]
fn marking_an_unknown_id_errors_and_writes_nothing() {
    let tmp = repo_with_one_intent();
    // A batch with a bad id marks nothing (atomic) and exits non-zero.
    aristo_in(tmp.path())
        .args(["review", "--mark", "a,bogus"])
        .assert()
        .failure()
        .stderr(contains("bogus"));

    // The valid id in the bad batch was NOT marked.
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 1"));
}

#[test]
fn editing_an_intent_after_review_reopens_it() {
    let tmp = repo_with_one_intent();
    aristo_in(tmp.path())
        .args(["review", "--mark", "a"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 0"));

    // Reword the claim (same id) and re-stamp → the reviewed snapshot drifts,
    // so the intent re-opens as unreviewed.
    write_lib(
        tmp.path(),
        r#"#[aristo::intent("does a DIFFERENT thing", verify = "test", id = "a")] fn a() {}"#,
    );
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 1"));
}

#[test]
fn review_is_empty_with_no_authored_intents() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_lib(tmp.path(), "pub fn a() {}\n");
    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["review", "--json"])
        .assert()
        .success()
        .stdout(contains("\"unreviewed_count\": 0"));
}
