//! Slice 27.5 step 6 — proof-review session smoke tests: start a
//! session against a fixture .proof file, decide a verdict /
//! suggested-annotation item, verify the substrate records the
//! per-kind fingerprint / snapshot correctly.

use assert_cmd::Command;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Write a minimal inconclusive .proof file with one suggested
/// annotation, so the per-kind handler has something to resolve.
fn write_fixture_inconclusive_proof(root: &Path, id: &str) {
    let dir = root.join(".aristo/proofs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{id}.proof"));
    let zero = format!("sha256:{}", "0".repeat(64));
    let body = format!(
        r#"[verdict]
type = "inconclusive"
method = "neural"
produced_at_text_hash = "{zero}"
produced_at_body_hash = "{zero}"
produced_by = "test"
attempts = 1
property_kind = "invariant"

[inconclusive.gap]
description = "missing assume about cell array bounds"
unfilled_path = "0.1"

[[inconclusive.gap.suggested_annotations]]
kind = "assume"
suggested_text = "Cell array slot count is bounded by page size."
at_site = "fn balance_non_root (src/btree.rs:142)"
rationale = "closes the unfilled step"
"#
    );
    std::fs::write(path, body).unwrap();
}

#[test]
fn reject_suggested_annotation_emits_per_kind_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_inconclusive_proof(tmp.path(), "balance_no_dup_cells");
    aristo_in(tmp.path())
        .args(["session", "start", "proof-review", "--subject", "fixture"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "balance_no_dup_cells#0",
            "--bucket",
            "rejected",
            "--note",
            "too narrow",
        ])
        .assert()
        .success();
    let log = std::fs::read_to_string(tmp.path().join(".aristo/sessions/rejections.log")).unwrap();
    assert!(
        log.contains("\"proof_id\":\"balance_no_dup_cells\""),
        "log: {log}"
    );
    assert!(log.contains("\"annotation_kind\":\"assume\""));
    assert!(log.contains("\"suggestion_text_hash\":\"sha256:"));
}

#[test]
fn accept_verdict_validates_ref_without_mutating_proof() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_inconclusive_proof(tmp.path(), "balance_no_dup_cells");
    let proof_path = tmp.path().join(".aristo/proofs/balance_no_dup_cells.proof");
    let before = std::fs::read_to_string(&proof_path).unwrap();
    aristo_in(tmp.path())
        .args(["session", "start", "proof-review", "--subject", "fixture"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "balance_no_dup_cells#verdict",
            "--bucket",
            "accepted",
        ])
        .assert()
        .success();
    let after = std::fs::read_to_string(&proof_path).unwrap();
    // Proof file is unchanged — accept is a no-op at the file level.
    assert_eq!(before, after);
}

#[test]
fn pending_suggestion_writes_backlog_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_inconclusive_proof(tmp.path(), "balance_no_dup_cells");
    aristo_in(tmp.path())
        .args(["session", "start", "proof-review", "--subject", "fixture"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "balance_no_dup_cells#0",
            "--bucket",
            "pending",
        ])
        .assert()
        .success();
    let backlog = std::fs::read_to_string(
        tmp.path()
            .join(".aristo/sessions/backlog/proof-review.toml"),
    )
    .unwrap();
    assert!(
        backlog.contains("Cell array slot count"),
        "backlog: {backlog}"
    );
    assert!(backlog.contains("annotation_kind"));
}

#[test]
fn out_of_range_suggestion_index_refused() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_inconclusive_proof(tmp.path(), "balance_no_dup_cells");
    aristo_in(tmp.path())
        .args(["session", "start", "proof-review", "--subject", "fixture"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "balance_no_dup_cells#99",
            "--bucket",
            "rejected",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no suggested_annotations"));
}
