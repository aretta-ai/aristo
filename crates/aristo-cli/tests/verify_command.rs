//! `aristo verify` (slice 22) — imperative tests for dispatcher
//! semantics that trycmd's `[..]`-wildcarded scenario can't pin down.
//!
//! Slice 22 ships the dispatcher + `verify = false` no-op arm; the
//! other verify methods return `NotImplemented` with their target
//! slice pointer.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
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
            "ok: 0 annotations verified, 0 skipped (documentation-only).",
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
            "ok: 0 annotations verified, 1 skipped (documentation-only).",
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
        .stdout(contains("/aristo-verify"));

    let task_path = tmp
        .path()
        .join(".aristo/verify-queue/pending/my_intent.toml");
    let task = fs::read_to_string(&task_path)
        .unwrap_or_else(|e| panic!("expected queue task at {:?}: {e}", task_path));
    assert!(
        task.contains("id = \"my_intent\""),
        "queue task must list the id; got:\n{task}"
    );
    assert!(
        task.contains("text_hash"),
        "queue task must include text_hash for the subagent to copy"
    );
}

#[test]
fn verify_test_intent_returns_not_implemented_with_plain_hint() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"test\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("is not implemented yet"))
        .stderr(contains("planned post-MVP"))
        // The hint must describe the limitation plainly — a repo-tree
        // doc path is un-followable for cargo-installed users.
        .stderr(contains("docs/deferred").not());
}

#[test]
fn verify_full_intent_returns_not_implemented_with_plain_hint() {
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_intent_at(tmp.path(), "verify = \"full\"");

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("is not implemented yet"))
        .stderr(contains("planned post-MVP"))
        .stderr(contains("docs/deferred").not());
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
            "ok: 0 annotations verified, 0 skipped (documentation-only).",
        ));

    // Filter HITS the entry — should count it.
    aristo_in(tmp.path())
        .args(["verify", "--filter", "id=my_intent"])
        .assert()
        .success()
        .stdout(contains(
            "ok: 0 annotations verified, 1 skipped (documentation-only).",
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
    // verify=false entries are always skipped (documentation-only),
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
            "ok: 0 annotations verified, 1 skipped (documentation-only).",
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
            "ok: 0 annotations verified, 1 skipped (documentation-only).",
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

#[test]
fn apply_verdicts_stamps_code_hash_when_agent_omitted_it() {
    // Agent-written proof with a Code ground that omits code_text_hash.
    // The validator accepts (existence + line-range checks pass), then
    // apply stamps the computed hash into the saved proof file.
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/x.rs"), "line1\nline2\nline3\n").unwrap();
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
claim = "by inspection"
relation_to_parent = "decomposes"
grounds = [{{ kind = "code", file = "src/x.rs", lines = "1-2", reason = "see literal" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success()
        .stdout(contains("applied: 1/1 verdict(s)"));

    let written = fs::read_to_string(tmp.path().join(".aristo/proofs/my_intent.proof")).unwrap();
    assert!(
        written.contains("code_text_hash = \"sha256:"),
        "expected stamped code_text_hash, got:\n{written}"
    );
}

#[test]
fn apply_verdicts_rewrite_hashes_migrates_proof_with_fabricated_hash() {
    // Simulates a pre-migration proof file: the agent wrote a wrong
    // (fabricated) code_text_hash. Without --rewrite-hashes the
    // validator rejects on staleness. With it, the stamped hash is
    // cleared, validated, and re-stamped.
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let fake_hash = format!("sha256:{}", "a".repeat(64));
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/x.rs"), "line1\nline2\nline3\n").unwrap();
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
claim = "by inspection"
relation_to_parent = "decomposes"
grounds = [{{ kind = "code", file = "src/x.rs", lines = "1-2", code_text_hash = "{fake_hash}", reason = "see literal" }}]
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);

    // Strict default: reject on stale stamped hash.
    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("code drifted since verification"));

    // Migration flag: clear + recompute + accept.
    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts", "--rewrite-hashes"])
        .assert()
        .success()
        .stdout(contains("applied: 1/1 verdict(s)"));

    let written = fs::read_to_string(tmp.path().join(".aristo/proofs/my_intent.proof")).unwrap();
    assert!(
        !written.contains(&fake_hash),
        "fabricated hash must be replaced post-migration; got:\n{written}"
    );
    assert!(
        written.contains("code_text_hash = \"sha256:"),
        "expected a stamped (real) code_text_hash post-migration; got:\n{written}"
    );
}

// ─── validator-at-list-time (skip logic consults validator) ────────────

#[test]
fn entry_with_valid_proof_is_skipped_on_subsequent_verify() {
    // After --apply-verdicts flips status to Neural AND stamps hashes
    // into the proof, the next `aristo verify` should consider this
    // entry done and not re-add it to pending.
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
        .success();

    // Next plain `verify` should skip — proof on disk + validator passes.
    aristo_in(tmp.path()).arg("verify").assert().success();

    assert!(
        !tmp.path()
            .join(".aristo/verify-queue/pending/my_intent.toml")
            .exists(),
        "no queue task should be enqueued when nothing's pending"
    );
}

#[test]
fn neural_only_workspace_with_fresh_proof_gets_no_zero_dispatch_warning() {
    // Review round 1, finding 2: a neural-only workspace whose entry
    // is skipped as clean has NOTHING to dispatch to the canon-verify
    // server — that is normal, not a vacuous-green hazard. The
    // zero-dispatch warning must count only canon-bound
    // `verify="full"` skips, so this run stays quiet.
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
        .success();

    // The entry is now skipped as clean. No canon-bound full entries
    // exist, so no zero-dispatch warning may fire.
    let assert = aristo_in(tmp.path()).arg("verify").assert().success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stderr.contains("no canon-verify dispatch"),
        "neural-only fresh-proof workspace must stay quiet, got: {stderr}"
    );
}

#[test]
fn counterexample_status_with_passing_proof_is_skipped() {
    // GAP-2: Counterexample used to be excluded from the terminal-clean
    // set, so it re-ran every verify cycle. The new skip logic should
    // treat it as terminal AS LONG AS the proof on disk still validates.
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
        .success();

    // status is now Counterexample. Next verify should NOT re-add to pending.
    aristo_in(tmp.path()).arg("verify").assert().success();
    assert!(
        !tmp.path()
            .join(".aristo/verify-queue/pending/my_intent.toml")
            .exists(),
        "counterexample with valid proof must not be re-pended (GAP-2 fix)"
    );
}

#[test]
fn terminal_status_without_proof_file_forces_reverify() {
    // Inconsistency case: index says terminal but no proof exists on disk.
    // The skip logic conservatively forces re-verification.
    let tmp = tempfile::tempdir().unwrap();
    workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property holds");
    // Manually flip status to Neural in the index without writing a proof.
    let idx_path = tmp.path().join(".aristo/index.toml");
    let idx = fs::read_to_string(&idx_path).unwrap();
    let idx = idx.replace("status = \"unknown\"", "status = \"neural\"");
    fs::write(&idx_path, idx).unwrap();

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stdout(contains("1 entry pending neural verification"));
}

// ─── GAP-9: attempts persistence across re-spawns ─────────────────────

#[test]
fn pending_carries_prior_attempts_from_existing_proof() {
    // A rejected proof for an entry leaves attempts=2 (say) on disk.
    // Next `aristo verify` must include `prior_attempts = 2` so the
    // skill's next subagent writes attempts=3, not attempts=1.
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "my_intent", "the property");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    // Write a proof with attempts=2 that will validate at hash-check
    // (focal hashes correct) but never gets accepted because we want
    // it to coexist with the verify-list-time check.
    let proof = format!(
        r#"[verdict]
type = "inconclusive"
method = "neural"
produced_at_text_hash = "{text_h}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 2
property_kind = "invariant"

[inconclusive.gap]
description = "stuck"
unfilled_path = "0"
[[inconclusive.gap.suggested_annotations]]
kind = "assume"
suggested_text = "something not in the index"
at_site = "fn x (line 1)"
rationale = "would close gap"
"#
    );
    write_proof(tmp.path(), "my_intent", &proof);
    // Apply once to flip status to Inconclusive.
    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success();

    // Now corrupt the proof so the validator-at-list-time check
    // rejects it (forcing re-pending), but the prior attempts read
    // still works.
    let path = tmp.path().join(".aristo/proofs/my_intent.proof");
    let raw = fs::read_to_string(&path).unwrap();
    // Replace produced_at_text_hash with garbage → validator rejects on
    // focal-text-hash mismatch at list time.
    let stale = format!("sha256:{}", "f".repeat(64));
    let raw = raw.replace(&text_h, &stale);
    fs::write(&path, raw).unwrap();

    aristo_in(tmp.path()).arg("verify").assert().success();
    let task_path = tmp
        .path()
        .join(".aristo/verify-queue/pending/my_intent.toml");
    let task = fs::read_to_string(&task_path)
        .unwrap_or_else(|e| panic!("expected queue task at {:?}: {e}", task_path));
    assert!(
        task.contains("prior_attempts = 2"),
        "queue task must carry prior_attempts; got:\n{task}"
    );
}

#[test]
fn pending_skips_and_warns_when_budget_exhausted() {
    // attempts=3 on the existing proof = budget exhausted. Next verify
    // must NOT include this id in pending (no point re-spawning to hit
    // attempts=4 which the validator would reject anyway) AND must warn
    // the user loudly on stderr.
    let tmp = tempfile::tempdir().unwrap();
    let text_h = workspace_with_one_neural_intent(tmp.path(), "stuck_intent", "the property");
    let zero_hash = format!("sha256:{}", "0".repeat(64));
    let proof = format!(
        r#"[verdict]
type = "inconclusive"
method = "neural"
produced_at_text_hash = "{text_h}"
produced_at_body_hash = "{zero_hash}"
produced_by = "test@0"
attempts = 3
property_kind = "invariant"

[inconclusive.gap]
description = "stuck"
unfilled_path = "0"
[[inconclusive.gap.suggested_annotations]]
kind = "assume"
suggested_text = "something not in the index"
at_site = "fn x (line 1)"
rationale = "would close gap"
"#
    );
    write_proof(tmp.path(), "stuck_intent", &proof);
    aristo_in(tmp.path())
        .args(["verify", "--apply-verdicts"])
        .assert()
        .success();

    // Corrupt the produced_at_text_hash → validator at list time rejects
    // → entry would re-pend, except prior_attempts=3 is at budget cap so
    // the SDK excludes it and warns instead.
    let path = tmp.path().join(".aristo/proofs/stuck_intent.proof");
    let raw = fs::read_to_string(&path).unwrap();
    let stale = format!("sha256:{}", "f".repeat(64));
    let raw = raw.replace(&text_h, &stale);
    fs::write(&path, raw).unwrap();

    aristo_in(tmp.path())
        .arg("verify")
        .assert()
        .success()
        .stderr(contains("exhausted the repair budget"))
        .stderr(contains("stuck_intent"));

    // Queue must NOT have a task file for the budget-exhausted entry.
    let task_path = tmp
        .path()
        .join(".aristo/verify-queue/pending/stuck_intent.toml");
    assert!(
        !task_path.exists(),
        "budget-exhausted entry must not be enqueued"
    );
}
