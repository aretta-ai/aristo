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
fn verify_neural_intent_writes_pending_request_file() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"neural\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stdout(contains("1 entry pending neural verification"))
        .stdout(contains("/aristo-neural-verify"));

    let pending = fs::read_to_string(tmp.path().join(".aristo/pending-neural.toml")).unwrap();
    assert!(
        pending.contains("id = \"my_intent\""),
        "pending file must list the entry; got:\n{pending}"
    );
    assert!(
        pending.contains("text_hash"),
        "pending file must include text_hash for the subagent to copy"
    );
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

// ─── --apply-verdicts (slice 23) ────────────────────────────────────────

fn write_proof(dir: &Path, id_filename_stem: &str, contents: &str) {
    let proofs = dir.join(".aristo/proofs");
    fs::create_dir_all(&proofs).unwrap();
    fs::write(proofs.join(format!("{id_filename_stem}.proof")), contents).unwrap();
}

/// Build a workspace + index with one verify=neural intent.
/// Returns the text_hash string the validator will check against.
fn workspace_with_one_neural_intent(dir: &Path, id: &str, text: &str) -> String {
    aristo_in(dir).arg("init").assert().success();
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let text_h = aristo_core::hash::text_hash(text).to_string();
    let index = format!(
        "[__meta__]\nschema_version = 1\n\n\
         [{id}]\nkind = \"intent\"\ntext = \"{text}\"\nverify = \"neural\"\nstatus = \"unknown\"\n\
         text_hash = \"{text_h}\"\nbody_hash = \"{zero_hash}\"\n\
         file = \"src/x.rs\"\nsite = \"fn x (line 1)\"\n\
         covered_region = \"function\"\n",
    );
    fs::write(dir.join(".aristo/index.toml"), index).unwrap();
    text_h
}

#[test]
fn apply_verdicts_with_no_proofs_dir_is_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success()
        .stdout(contains("no pending verdict files"));
}

#[test]
fn apply_verdicts_accepts_valid_verified_proof_and_flips_status() {
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let proof = format!(
        r#"[verdict]
type = "verified"
method = "neural"
produced_at_text_hash = "{text_h}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 1
property_kind = "invariant"

[verified.proof]
conclusion = "the property holds"

[[verified.proof.steps]]
path = "0"
claim = "trivially"
relation_to_parent = "decomposes"
grounds = [{{ kind = "composition", reason = "single-step proof" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success()
        .stdout(contains("applied: 1/1 verdict(s)"));

    aristo_in(tmp.path())
        .args(["show", "my_intent"])
        .assert()
        .success()
        .stdout(contains("status:"))
        .stdout(contains("neural"));
}

#[test]
fn apply_verdicts_rejects_stale_text_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let _text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let stale = format!("sha256:{}", "f".repeat(64));
    let proof = format!(
        r#"[verdict]
type = "verified"
method = "neural"
produced_at_text_hash = "{stale}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 1
property_kind = "invariant"

[verified.proof]
conclusion = "x"

[[verified.proof.steps]]
path = "0"
claim = "trivially"
relation_to_parent = "decomposes"
grounds = [{{ kind = "composition", reason = "trivial" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("text drifted since verification"));
}

#[test]
fn apply_verdicts_rejects_attempts_over_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let proof = format!(
        r#"[verdict]
type = "verified"
method = "neural"
produced_at_text_hash = "{text_h}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 99
property_kind = "invariant"

[verified.proof]
conclusion = "x"

[[verified.proof.steps]]
path = "0"
claim = "trivially"
relation_to_parent = "decomposes"
grounds = [{{ kind = "composition", reason = "trivial" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("exceeds budget"));
}

#[test]
fn apply_verdicts_counterexample_flips_to_counterexample_status() {
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let proof = format!(
        r#"[verdict]
type = "counterexample"
method = "neural"
produced_at_text_hash = "{text_h}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 1
property_kind = "invariant"

[counterexample.violation]
description = "the property fails when input is empty"
violated_step_path = "0"

[[counterexample.violation.trigger_steps]]
path = "0"
claim = "empty input bypasses the check"
relation_to_parent = "decomposes"
grounds = [{{ kind = "composition", reason = "by inspection of source" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success()
        .stdout(contains("applied: 1/1 verdict(s)"));

    aristo_in(tmp.path())
        .args(["show", "my_intent"])
        .assert()
        .success()
        .stdout(contains("counterexample"));
}

#[test]
fn apply_verdicts_rejects_unparseable_proof() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property");
    write_proof(tmp.path(), "my_intent", "this is not valid TOML at all {");

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("parse:"));
}
