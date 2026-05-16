//! `aristo verify` (slice 22) — imperative tests for dispatcher
//! semantics that trycmd's `[..]`-wildcarded scenario can't pin down.
//!
//! Slice 22 ships the dispatcher + `verify = false` no-op arm; the
//! other verify methods return `NotImplemented` with their target
//! slice pointer.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Hand-craft an index with one intent at the given verify level.
/// Tests use this instead of going through stamp so the workspace
/// shape is predictable and doesn't depend on the walker.
fn workspace_with_one_intent_at(dir: &Path, verify_line: &str) {
    aristo_in(dir).arg("init").assert().success();
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let index = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [my_intent]\nkind = \"intent\"\ntext = \"the property\"\n\
         {verify_line}\nstatus = \"unknown\"\n\
         text_hash = \"{zero_hash}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"src/x.rs\"\nsite = \"fn x (line 1)\"\n\
         covered_region = \"function\"\n",
    );
    fs::write(dir.join(".aristo/index.toml"), index).unwrap();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn empty_workspace_reports_zero_verified_zero_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 0 skipped (documentation only).",
        ));
}

#[test]
fn verify_false_intent_is_counted_as_skipped_documentation_only() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = false");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 1 skipped (documentation only).",
        ));
}

#[test]
fn verify_neural_intent_returns_not_implemented_with_slice_23() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"neural\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("not yet implemented"))
        .stderr(contains("slice 23"));
}

#[test]
fn verify_test_intent_returns_not_implemented_with_slice_24() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"test\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("not yet implemented"))
        .stderr(contains("slice 24"));
}

#[test]
fn verify_full_intent_returns_not_implemented_with_slice_26() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"full\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("not yet implemented"))
        .stderr(contains("slice 26"));
}

#[test]
fn filter_id_narrows_to_one_entry() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = false");

    // Filter MISSES the only entry — should report zero skipped.
    aristo_in(tmp.path())
        .args(["verify", "--filter", "id=nonexistent"])
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 0 skipped (documentation only).",
        ));

    // Filter HITS the entry — should count it.
    aristo_in(tmp.path())
        .args(["verify", "--filter", "id=my_intent"])
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 1 skipped (documentation only).",
        ));
}

#[test]
fn unknown_filter_key_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = false");

    aristo_in(tmp.path())
        .args(["verify", "--filter", "kind=intent"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("unknown filter key"));
}

#[test]
fn rerun_does_not_reverify_verify_false_entries() {
    // verify=false entries are always skipped (documentation only),
    // even under --rerun. --rerun forces re-processing of clean
    // verified entries (Status::Verified|Tested|Neural), not of
    // intentional opt-outs.
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = false");

    aristo_in(tmp.path())
        .args(["verify", "--rerun"])
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 1 skipped (documentation only).",
        ));
}

#[test]
fn assume_entries_are_treated_as_documentation_only() {
    // Assume entries have no `verify` field by design — they describe
    // external trust. The dispatcher resolves them to Bool(false) so
    // they take the same docs-only skip path as opt-out intents.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let index = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [my_assume]\nkind = \"assume\"\ntext = \"OS guarantees X\"\n\
         status = \"unknown\"\n\
         text_hash = \"{zero_hash}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"src/x.rs\"\nsite = \"fn x (line 1)\"\n\
         covered_region = \"function\"\n",
    );
    fs::write(tmp.path().join(".aristo/index.toml"), index).unwrap();

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 1 skipped (documentation only).",
        ));
}
