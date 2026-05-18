//! `aristo session` (slice 27.5 step 2) — full-lifecycle imperative
//! tests. trycmd can't easily express multi-step state mutations on
//! shared workspace fixtures with re-decision and forced-close
//! semantics; an imperative test is the right fit.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn init_workspace(dir: &Path) {
    aristo_in(dir).arg("init").assert().success();
}

#[test]
fn errors_outside_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path())
        .args(["session", "active"])
        .assert()
        .failure()
        .stderr(contains("not inside an Aristo workspace"));
}

#[test]
fn active_with_no_session_prints_nothing_exit_zero() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "active"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn status_with_no_session_errors_out() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "status"])
        .assert()
        .failure()
        .stderr(contains("no active session"));
}

#[test]
fn start_creates_session_and_active_prints_id() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args([
            "session",
            "start",
            "critique-review",
            "--subject",
            "src/foo.rs",
        ])
        .assert()
        .success()
        .stdout(contains("ok: started session"));
    let out = aristo_in(tmp.path())
        .args(["session", "active"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.trim().len() >= 23,
        "expected session id, got {stdout:?}"
    );
}

#[test]
fn start_refuses_when_session_already_active() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "start", "proof-review", "--subject", "y"])
        .assert()
        .failure()
        .stderr(contains("a session is already active"));
}

#[test]
fn decide_records_buckets_and_status_reflects_counts() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    for (item, bucket) in [
        ("foo#0", "accepted"),
        ("foo#1", "rejected"),
        ("foo#2", "pending"),
        ("foo#3", "accepted"),
    ] {
        aristo_in(tmp.path())
            .args(["session", "decide", "--item", item, "--bucket", bucket])
            .assert()
            .success();
    }
    aristo_in(tmp.path())
        .args(["session", "status"])
        .assert()
        .success()
        .stdout(contains("2 accepted"))
        .stdout(contains("1 rejected"))
        .stdout(contains("1 pending"));
}

#[test]
fn redecide_is_idempotent_and_replaces_prior_bucket() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session", "decide", "--item", "foo#0", "--bucket", "accepted",
        ])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session", "decide", "--item", "foo#0", "--bucket", "rejected",
        ])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "status"])
        .assert()
        .success()
        .stdout(contains("0 accepted"))
        .stdout(contains("1 rejected"));
}

#[test]
fn exit_strict_refuses_when_items_open() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    // Manually create an open item by editing the session file —
    // the CLI's `decide` path only produces decided items. We
    // simulate the per-kind start flow's "seed open items" step by
    // appending one to the session file via the substrate.
    seed_open_item(tmp.path(), "open#0");
    aristo_in(tmp.path())
        .args(["session", "exit"])
        .assert()
        .failure()
        .stderr(contains("item(s) still open"));
}

#[test]
fn exit_defer_undecided_moves_open_items_to_backlog_and_closes() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    seed_open_item(tmp.path(), "open#0");
    seed_open_item(tmp.path(), "open#1");
    aristo_in(tmp.path())
        .args(["session", "exit", "--defer-undecided"])
        .assert()
        .success()
        .stdout(contains("2 pending"));
    // Active session should be gone.
    aristo_in(tmp.path())
        .args(["session", "active"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
    // Backlog file should exist with 2 items.
    let backlog = tmp
        .path()
        .join(".aristo/sessions/backlog/critique-review.toml");
    assert!(backlog.exists(), "backlog file should be written");
    let body = std::fs::read_to_string(&backlog).unwrap();
    assert!(body.contains("open#0"));
    assert!(body.contains("open#1"));
}

#[test]
fn exit_strict_succeeds_when_all_items_decided() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args([
            "session", "decide", "--item", "foo#0", "--bucket", "accepted",
        ])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "exit"])
        .assert()
        .success()
        .stdout(contains("closed session"));
}

#[test]
fn abort_with_yes_drops_session_without_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "abort", "--yes"])
        .assert()
        .success()
        .stdout(contains("aborted session"));
    aristo_in(tmp.path())
        .args(["session", "active"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn rejected_decision_appends_to_rejection_log() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
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
            "intentionally narrative",
        ])
        .assert()
        .success();
    let log = tmp.path().join(".aristo/sessions/rejections.log");
    assert!(log.exists(), "rejections.log should be written");
    let body = std::fs::read_to_string(&log).unwrap();
    assert!(body.contains("\"item_ref\":\"foo#0\""));
    assert!(body.contains("\"kind\":\"critique-review\""));
    assert!(body.contains("intentionally narrative"));
}

#[test]
fn hook_format_emits_system_reminder_block_when_active() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "x"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "active", "--hook-format"])
        .assert()
        .success()
        .stdout(contains("<system-reminder>"))
        .stdout(contains("aristo session decide"))
        .stdout(contains("</system-reminder>"));
}

#[test]
fn hook_format_emits_nothing_when_no_session() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    aristo_in(tmp.path())
        .args(["session", "active", "--hook-format"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn list_shows_active_and_closed_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    init_workspace(tmp.path());
    // Close one session.
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "first"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "abort", "--yes"])
        .assert()
        .success();
    // Start another and leave it open.
    aristo_in(tmp.path())
        .args(["session", "start", "critique-review", "--subject", "second"])
        .assert()
        .success();
    aristo_in(tmp.path())
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(contains("subject=second"))
        .stdout(contains("kind=critique-review"));
}

// ─── helpers ─────────────────────────────────────────────────────────

/// Inject an open item into the active session file. The `decide`
/// CLI only creates decided items; this simulates what per-kind
/// `start` will do in step 5 when it seeds the session with the
/// candidate items it found via filtering. We round-trip through
/// `toml::Value` so we don't need to know whether the source had
/// an empty `items = []` or no items key at all.
fn seed_open_item(workspace_root: &Path, item_ref: &str) {
    let active_dir = workspace_root.join(".aristo/sessions/active");
    let file = std::fs::read_dir(&active_dir)
        .unwrap()
        .next()
        .expect("active session file should exist")
        .unwrap();
    let path = file.path();
    let body = std::fs::read_to_string(&path).unwrap();
    let mut value: toml::Value = toml::from_str(&body).unwrap();
    let items = value
        .as_table_mut()
        .unwrap()
        .entry("items")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();
    let mut item = toml::value::Table::new();
    item.insert("ref".to_string(), toml::Value::String(item_ref.to_string()));
    item.insert(
        "status".to_string(),
        toml::Value::String("open".to_string()),
    );
    items.push(toml::Value::Table(item));
    std::fs::write(&path, toml::to_string(&value).unwrap()).unwrap();
}
