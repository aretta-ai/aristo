//! §17 Slice 3 — adaptive modes (§6B).
//!
//! Modes are recipes of EXISTING CLI primitives — no new grammar. This
//! suite pins that each mode's underlying flag behaves as the skill
//! (Slice 4) relies on:
//!
//! - **backlog** — `aristo stamp --skip-canon` runs no match (offline;
//!   the session then seeds purely from cached/backlog state).
//! - **cluster `<objective>`** — `aristo canon suggestions --filter
//!   parent=<obj>` scopes to one proof cluster (reuses `filter.rs`).
//! - **status** — `aristo canon suggestions --counts` prints counts with
//!   NO session and NO writes.
//! - **matches / suggestions** — `aristo canon list` (matches read) and
//!   `aristo canon suggestions` (suggestions read) each surface only one
//!   stage.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use std::path::Path;

fn aristo_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Seed a queued cluster task (objective + one sibling) directly.
fn seed_cluster(root: &Path, key: &str, sibling: &str) {
    let dir = root.join(".aristo/canon-suggestions-queue/pending");
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "for_canon_ids = [\"primary_for_{key}\"]\n\
         discovered_at = \"2026-06-05T00:00:00Z\"\n\n\
         [objective]\ncanon_id = \"{key}\"\nversion = \"v0.1.0\"\n\
         canonical_text = \"objective text\"\nscope = \"turso\"\n\
         prefix_tier = \"kanon:\"\nbacked_by = \"\"\ndisposition = \"open\"\n\n\
         [[siblings]]\ncanon_id = \"{sibling}\"\nversion = \"v0.1.0\"\n\
         canonical_text = \"sibling text\"\nscope = \"turso\"\n\
         prefix_tier = \"aristos:\"\nbacked_by = \"golden model\"\n\
         disposition = \"open\"\n"
    );
    std::fs::write(dir.join(format!("{key}.toml")), body).unwrap();
}

const SOURCE: &str = r#"
#[aristo::intent(
    "a commit frame must reach stable storage via fsync before durable",
    id = "wal_commit_durable_invariant"
)]
pub fn commit() {}
"#;

fn setup_source(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), SOURCE).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"sandbox\"\nversion = \"0.0.1\"\nedition = \"2021\"\n",
    )
    .unwrap();
}

#[test]
fn skip_canon_mode_runs_no_match() {
    // backlog mode: `--skip-canon` means no `/canon/match` call — no
    // network, no cache, no suggestions queue produced. With no canon
    // fixture set, a non-skip stamp would attempt a match; --skip-canon
    // makes the run offline.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    setup_source(tmp.path());

    aristo_in(tmp.path())
        .args(["stamp", "--skip-canon"])
        .assert()
        .success()
        .stdout(predicates::str::contains("canon-match: skipped"));

    // No suggestions queue and no cache were produced.
    assert!(
        !tmp.path()
            .join(".aristo/canon-suggestions-queue/pending")
            .exists()
            || std::fs::read_dir(tmp.path().join(".aristo/canon-suggestions-queue/pending"))
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
        "skip-canon must not populate the suggestions queue"
    );
    assert!(
        !tmp.path().join(".aristo/canon-matches.toml").exists(),
        "skip-canon must not write the match cache"
    );
}

#[test]
fn filter_parent_scopes_to_one_cluster() {
    // cluster <objective> mode: `--filter parent=<obj>` (reusing the
    // shared filter grammar verbatim) scopes the listing to exactly that
    // cluster.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        "wal_find_frame_range_invariant",
    );
    seed_cluster(
        tmp.path(),
        "storage_compaction_correctness",
        "vacuum_atomic_under_crash",
    );

    // Unfiltered: both clusters.
    aristo_in(tmp.path())
        .args(["canon", "suggestions"])
        .assert()
        .success()
        .stdout(predicates::str::contains("2 cluster(s)"));

    // Filtered: just the WAL cluster.
    aristo_in(tmp.path())
        .args([
            "canon",
            "suggestions",
            "--filter",
            "parent=wal_protocol_correctness",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("1 cluster(s)"))
        .stdout(predicates::str::contains("wal_protocol_correctness"))
        .stdout(predicates::str::contains("storage_compaction_correctness").not());
}

#[test]
fn filter_parent_accepts_kanon_prefixed_objective() {
    // The §6B `cluster <objective>` recipe is `--filter
    // parent=kanon:<objective>`; the tier prefix is stripped so it
    // matches the bare cluster key.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        "wal_find_frame_range_invariant",
    );

    aristo_in(tmp.path())
        .args([
            "canon",
            "suggestions",
            "--filter",
            "parent=kanon:wal_protocol_correctness",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("1 cluster(s)"))
        .stdout(predicates::str::contains("wal_protocol_correctness"));
}

#[test]
fn queue_status_counts_emit_no_session_no_writes() {
    // status mode: `--counts` prints the counts table and exits — no
    // session is opened, nothing is mutated.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        "wal_find_frame_range_invariant",
    );

    let out = aristo_in(tmp.path())
        .args(["canon", "suggestions", "--counts"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(v["suggestions"]["new"], 1, "got: {v}");

    // No session was started.
    assert!(
        !tmp.path().join(".aristo/sessions/.active").exists(),
        "status mode must not open a session"
    );
}

#[test]
fn matches_only_and_suggestions_only_each_skip_the_other_stage() {
    // matches / suggestions stage selectors: `aristo canon list` reads
    // only the primary-match cache; `aristo canon suggestions` reads only
    // the suggestions queue. Each surfaces its own stage and not the
    // other.
    let tmp = tempfile::tempdir().unwrap();
    aristo_in(tmp.path()).arg("init").assert().success();
    seed_cluster(
        tmp.path(),
        "wal_protocol_correctness",
        "wal_find_frame_range_invariant",
    );

    // suggestions-only: the queue, not the (empty) match cache.
    aristo_in(tmp.path())
        .args(["canon", "suggestions"])
        .assert()
        .success()
        .stdout(predicates::str::contains("proof-tree suggestions"));

    // matches-only: the match cache, which has nothing — and critically
    // does NOT surface the queued suggestion cluster.
    aristo_in(tmp.path())
        .args(["canon", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no canon matches"))
        .stdout(predicates::str::contains("wal_protocol_correctness").not());
}
