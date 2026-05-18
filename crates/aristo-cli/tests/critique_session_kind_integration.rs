//! Slice 27.5 step 5 — end-to-end critique-review session: start a
//! session, decide one finding via the CLI, verify the .critique
//! file's disposition was stamped accordingly.

use assert_cmd::Command;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Write a minimal .critique file with two findings so the test
/// doesn't need the full critique pipeline to run.
fn write_fixture_critique(root: &Path, id: &str) {
    let dir = root.join(".aristo/critiques");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{id}.critique"));
    let zero = format!("sha256:{}", "0".repeat(64));
    let body = format!(
        r#"[critique]
critiqued_at_text_hash = "{zero}"
produced_at_body_hash = "{zero}"
produced_by = "test"
attempts = 1
finding_count = 2
highest_severity = "suggest"

[[critique.findings]]
category = "clarity"
severity = "suggest"
rationale = "defensive commentary"

[[critique.findings]]
category = "rephrasing"
severity = "strong-suggest"
rationale = "double negation"
suggested_text = "rewrite"
"#
    );
    std::fs::write(path, body).unwrap();
}

#[test]
fn accept_decision_stamps_disposition_in_critique_file() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_critique(tmp.path(), "foo");

    aristo_in(tmp.path())
        .args([
            "session",
            "start",
            "critique-review",
            "--subject",
            "test fixture",
        ])
        .assert()
        .success();

    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "foo#1",
            "--bucket",
            "accepted",
            "--note",
            "rewrite looks right",
        ])
        .assert()
        .success();

    let body = std::fs::read_to_string(tmp.path().join(".aristo/critiques/foo.critique")).unwrap();
    assert!(body.contains("disposition = \"accepted\""), "body: {body}");
    assert!(body.contains("rewrite looks right"));
    // Untouched finding stays disposition-less.
    let foo_section = body.split("[[critique.findings]]").nth(1).unwrap();
    assert!(
        !foo_section.contains("disposition"),
        "first finding shouldn't have disposition: {foo_section}"
    );
}

#[test]
fn reject_decision_appends_rejection_with_per_kind_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_critique(tmp.path(), "foo");
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "foo#0",
            "--bucket",
            "rejected",
            "--note",
            "narrative",
        ])
        .assert()
        .success();

    let log_path = tmp.path().join(".aristo/sessions/rejections.log");
    let log = std::fs::read_to_string(&log_path).unwrap();
    // Per-kind fingerprint: category + rationale_sketch.
    assert!(log.contains("\"category\":\"clarity\""), "log: {log}");
    assert!(
        log.contains("\"rationale_sketch\":\"defensive commentary\""),
        "log: {log}"
    );
    // The .critique file also gets disposition = rejected.
    let body = std::fs::read_to_string(tmp.path().join(".aristo/critiques/foo.critique")).unwrap();
    assert!(body.contains("disposition = \"rejected\""));
}

#[test]
fn pending_decision_writes_backlog_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    write_fixture_critique(tmp.path(), "foo");
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session", "decide", "--item", "foo#1", "--bucket", "pending", "--note", "later",
        ])
        .assert()
        .success();
    let backlog = std::fs::read_to_string(
        tmp.path()
            .join(".aristo/sessions/backlog/critique-review.toml"),
    )
    .unwrap();
    // Backlog data captures finding snapshot.
    assert!(backlog.contains("rephrasing"), "backlog: {backlog}");
    assert!(backlog.contains("strong-suggest"));
    let critique =
        std::fs::read_to_string(tmp.path().join(".aristo/critiques/foo.critique")).unwrap();
    assert!(critique.contains("disposition = \"deferred\""));
}

#[test]
fn missing_critique_file_refused_with_actionable_error() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "never_submitted#0",
            "--bucket",
            "accepted",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no .critique file"));
}

#[test]
fn bad_ref_form_refused_with_actionable_error() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "no_hash_here",
            "--bucket",
            "accepted",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not in"));
}
