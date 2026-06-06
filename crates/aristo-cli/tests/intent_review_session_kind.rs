//! §17 Slice 3 — end-to-end `intent-review` session kind.
//!
//! The session carries two item types: pending PRIMARY matches
//! (`match:<id>#<canon-id>`) and SUGGESTION clusters (`cluster:<key>`)
//! with their siblings (`sibling:<key>#<canon-id>`). Parent decided
//! first (D6); reject-parent discards the cluster's DRAGGED-IN siblings
//! only and KEEPS any member that independently matched on its own.
//!
//! Tests drive the real `aristo session {start,decide,exit}` CLI against
//! a hand-seeded suggestions queue + cache, mirroring
//! `critique_session_kind_integration.rs`.

use assert_cmd::Command;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Seed a queued cluster task (objective + the given siblings) directly
/// into `.aristo/canon-suggestions-queue/pending/<key>.toml`. The on-disk
/// shape matches what `runner.rs` writes after a stamp (verified against
/// a real stamp run).
fn seed_cluster(root: &Path, key: &str, siblings: &[&str]) {
    let dir = root.join(".aristo/canon-suggestions-queue/pending");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::new();
    body.push_str("for_canon_ids = [\"wal_commit_requires_fsync\"]\n");
    body.push_str("discovered_at = \"2026-06-05T00:00:00Z\"\n\n");
    body.push_str("[objective]\n");
    body.push_str(&format!("canon_id = \"{key}\"\n"));
    body.push_str("version = \"v0.1.0\"\n");
    body.push_str("canonical_text = \"the WAL subsystem maintains correctness\"\n");
    body.push_str("scope = \"turso\"\n");
    body.push_str("prefix_tier = \"kanon:\"\n");
    body.push_str("backed_by = \"\"\n");
    body.push_str("disposition = \"open\"\n");
    for s in siblings {
        body.push_str("\n[[siblings]]\n");
        body.push_str(&format!("canon_id = \"{s}\"\n"));
        body.push_str("version = \"v0.1.0\"\n");
        body.push_str(&format!("canonical_text = \"text for {s}\"\n"));
        body.push_str("scope = \"turso\"\n");
        body.push_str("prefix_tier = \"aristos:\"\n");
        body.push_str("backed_by = \"golden model\"\n");
        body.push_str("disposition = \"open\"\n");
    }
    std::fs::write(dir.join(format!("{key}.toml")), body).unwrap();
}

/// Add a pending PRIMARY match for `canon_id` to `canon-matches.toml`
/// under a synthetic annotation. This is the "independently held"
/// signal the D6 guard re-checks at discard time — a member that
/// *became* a primary after the cluster was queued.
fn seed_pending_match(root: &Path, annotation_id: &str, canon_id: &str) {
    let path = root.join(".aristo/canon-matches.toml");
    let existing = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        "[__meta__]\nschema_version = 1\ncanon_version = \"v0.2.0\"\n\
         last_fetched = \"2026-06-05T00:00:00Z\"\n"
            .to_string()
    });
    let zero = format!("sha256:{}", "0".repeat(64));
    let appended = format!(
        "{existing}\n[{annotation_id}]\nlast_match_text_hash = \"{zero}\"\n\
         canon_fetched_at = \"2026-06-05T00:00:00Z\"\n\n\
         [[{annotation_id}.pending_matches]]\ncanon_id = \"{canon_id}\"\n\
         version = \"v0.1.0\"\ncanonical_text = \"text for {canon_id}\"\n\
         canon_version = \"v0.2.0\"\nconfidence = 0.9\nprefix_tier = \"aristos:\"\n\
         backed_by = \"golden model\"\ndisposition = \"open\"\n\
         found_at = \"2026-06-05T00:00:00Z\"\nfound_by = \"aristo stamp\"\n"
    );
    std::fs::write(&path, appended).unwrap();
}

fn read_queue_task(root: &Path, key: &str) -> Option<String> {
    let p = root.join(format!(
        ".aristo/canon-suggestions-queue/pending/{key}.toml"
    ));
    std::fs::read_to_string(p).ok()
}

#[test]
fn intent_review_is_a_registered_session_kind() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success()
        .stdout(predicates::str::contains("kind=intent-review"));
}

#[test]
fn parent_first_then_accept_parent_opens_siblings() {
    // Accept-parent is a validating no-op (adoption goes through
    // stamp→accept per D4); the cluster stays queued so siblings can be
    // decided individually afterward.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        &["wal_find_frame_range_invariant", "wal_checkpoint_error_no_db_leak"],
    );
    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success();

    // Decide the PARENT first.
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "cluster:wal_protocol_correctness",
            "--bucket",
            "accepted",
        ])
        .assert()
        .success();

    // Siblings are still reviewable individually.
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "sibling:wal_protocol_correctness#wal_find_frame_range_invariant",
            "--bucket",
            "accepted",
        ])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "sibling:wal_protocol_correctness#wal_checkpoint_error_no_db_leak",
            "--bucket",
            "rejected",
        ])
        .assert()
        .success();

    // The rejected sibling is fingerprinted by its bare canon_id.
    let log = std::fs::read_to_string(tmp.path().join(".aristo/sessions/rejections.log")).unwrap();
    assert!(
        log.contains("\"fingerprint\":\"wal_checkpoint_error_no_db_leak\""),
        "sibling reject must fingerprint its canon_id; log: {log}"
    );
}

#[test]
fn reject_parent_keeps_independent_match() {
    // THE load-bearing D6 test. A cluster has two dragged-in siblings.
    // One of them (`wal_checkpoint_error_no_db_leak`) independently
    // matched on its own AFTER the cluster was queued — it has a pending
    // primary match in the cache. Rejecting the parent must:
    //   - DISCARD the purely-dragged-in sibling
    //     (`wal_find_frame_range_invariant`) + fingerprint it, AND
    //   - KEEP the independently-matched sibling in the queue (never
    //     silently throw away an invariant the user is asserting).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        &["wal_find_frame_range_invariant", "wal_checkpoint_error_no_db_leak"],
    );
    // The second sibling became a primary after merge.
    seed_pending_match(
        tmp.path(),
        "some_other_annotation",
        "wal_checkpoint_error_no_db_leak",
    );

    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "cluster:wal_protocol_correctness",
            "--bucket",
            "rejected",
            "--note",
            "not this contract",
        ])
        .assert()
        .success();

    // The task still exists (the independent member survives).
    let task = read_queue_task(tmp.path(), "wal_protocol_correctness")
        .expect("task must survive — it still holds an independently-matched member");
    // The purely-dragged-in sibling is gone…
    assert!(
        !task.contains("wal_find_frame_range_invariant"),
        "dragged-in-only sibling must be discarded; task: {task}"
    );
    // …the independently-matched member is KEPT…
    assert!(
        task.contains("wal_checkpoint_error_no_db_leak"),
        "independently-matched member must be KEPT (D6); task: {task}"
    );
    // …and the objective is dropped (the parent was rejected).
    assert!(
        !task.contains("[objective]"),
        "rejected parent's objective must be dropped; task: {task}"
    );

    // The discarded member is fingerprinted; the kept member is NOT
    // (it keeps its own accept/reject flow).
    let log = std::fs::read_to_string(tmp.path().join(".aristo/sessions/rejections.log")).unwrap();
    assert!(
        log.contains("\"fingerprint\":\"wal_find_frame_range_invariant\""),
        "discarded sibling must be fingerprinted; log: {log}"
    );
    assert!(
        !log.contains("\"item_ref\":\"wal_checkpoint_error_no_db_leak\""),
        "the independently-matched member must NOT be discarded/fingerprinted; log: {log}"
    );
}

#[test]
fn reject_parent_with_no_independent_member_removes_the_whole_task() {
    // When every sibling is purely dragged in, rejecting the parent
    // removes the whole cluster task (nothing the user independently
    // holds), fingerprinting each discarded member.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        &["wal_find_frame_range_invariant", "wal_checkpoint_error_no_db_leak"],
    );
    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "cluster:wal_protocol_correctness",
            "--bucket",
            "rejected",
        ])
        .assert()
        .success();

    assert!(
        read_queue_task(tmp.path(), "wal_protocol_correctness").is_none(),
        "task with no independent member must be removed entirely"
    );
    let log = std::fs::read_to_string(tmp.path().join(".aristo/sessions/rejections.log")).unwrap();
    assert!(log.contains("\"fingerprint\":\"wal_find_frame_range_invariant\""));
    assert!(log.contains("\"fingerprint\":\"wal_checkpoint_error_no_db_leak\""));
}

#[test]
fn exit_defer_undecided_parks_remaining_items() {
    // `exit --defer-undecided` parks remaining open items in the per-kind
    // backlog rather than erroring (strict exit would reject open items).
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        &["wal_find_frame_range_invariant"],
    );
    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success();
    // Defer one sibling; leave it parked.
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "sibling:wal_protocol_correctness#wal_find_frame_range_invariant",
            "--bucket",
            "pending",
            "--note",
            "later",
        ])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "exit", "--defer-undecided"])
        .assert()
        .success();

    let backlog = std::fs::read_to_string(
        tmp.path().join(".aristo/sessions/backlog/intent-review.toml"),
    )
    .unwrap();
    assert!(
        backlog.contains("wal_find_frame_range_invariant"),
        "deferred sibling must be parked in the intent-review backlog; backlog: {backlog}"
    );
}

#[test]
fn bad_intent_ref_refused_with_actionable_error() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    aristo_in(tmp.path())
        .args(["session", "start", "intent-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session",
            "decide",
            "--item",
            "not_a_valid_intent_ref",
            "--bucket",
            "accepted",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("is not in"));
}
