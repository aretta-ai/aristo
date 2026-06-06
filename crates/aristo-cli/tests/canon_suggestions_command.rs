//! End-to-end scenario tests for `aristo canon suggestions` (§17
//! Slice 2). Pattern mirrors the other `canon_*` command e2e tests.
//!
//! The flow: a workspace with one annotation → `aristo stamp` against a
//! mock canon fixture whose `/canon/match` response carries a §17
//! `suggestions` cluster → the runner routes the cluster into the
//! `.aristo/canon-suggestions-queue/` (dedup ②③) → `aristo canon
//! suggestions {list, <objective>, --counts}` reads it back.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

fn aristo_in(workspace: &Path) -> Command {
    let mut c = Command::new(aristo_bin());
    c.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        c.env("PATH", path);
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("DYLD_FALLBACK_LIBRARY_PATH") {
        c.env("DYLD_FALLBACK_LIBRARY_PATH", p);
    }
    let home = workspace.join("home");
    std::fs::create_dir_all(&home).unwrap();
    c.env("HOME", &home);
    c.env("XDG_CONFIG_HOME", home.join("xdg"));
    c.current_dir(workspace);
    c
}

fn setup_workspace(source_body: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("aristo.toml"), "").unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".aristo")).unwrap();
    std::fs::write(tmp.path().join("src").join("lib.rs"), source_body).unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"sandbox\"\nversion = \"0.0.1\"\nedition = \"2021\"\n",
    )
    .unwrap();
    tmp
}

/// A `/canon/match` mock fixture: one primary match for the annotation,
/// plus a §17 suggestions cluster (objective + two siblings). One of the
/// siblings (`wal_commit_requires_fsync`) is the primary itself in the
/// real golden; here the siblings are distinct co-members to exercise
/// the queue path. Written as TOML (the mock client's wire format).
fn write_match_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = ["turso", ":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-05T00:00:00Z"

results = [
    [
        { canon_id = "wal_commit_requires_fsync", version = "v0.1.0", canonical_text = "a commit frame must reach stable storage via fsync", confidence = 0.91, scope = "turso", prefix_tier = "aristos:", backed_by = "golden model + proofs + differential testing", linked = "arta_5b1c9f2a7e3d4068", verification = { coverage_level = "tight", test_binaries = ["wal_conform"] } }
    ]
]

[[suggestions]]
for_canon_id = "wal_commit_requires_fsync"
objective = { canon_id = "wal_protocol_correctness", version = "v0.1.0", canonical_text = "the WAL subsystem maintains LSN monotonicity and frame commitment ordering", scope = "turso", prefix_tier = "kanon:", backed_by = "", relationship = "parent" }
siblings = [
    { canon_id = "wal_find_frame_range_invariant", version = "v0.1.0", canonical_text = "find_frame never reads outside the live frame range", scope = "turso", prefix_tier = "aristos:", backed_by = "golden model + proofs + differential testing", relationship = "sibling" },
    { canon_id = "wal_checkpoint_error_no_db_leak", version = "v0.1.0", canonical_text = "a checkpoint failure must not leak frames into the main database file", scope = "turso", prefix_tier = "aristos:", backed_by = "golden model + proofs + differential testing", relationship = "sibling" }
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

const SOURCE: &str = r#"
#[aristo::intent(
    "a commit frame must reach stable storage via fsync before durable",
    id = "wal_commit_durable_invariant"
)]
pub fn commit() {}
"#;

/// Run `aristo stamp` against the mock fixture so the suggestions queue
/// is populated. Returns the workspace dir.
fn stamped_workspace() -> TempDir {
    let ws = setup_workspace(SOURCE);
    let fixture_dir = ws.path().join("fixture");
    write_match_fixture(&fixture_dir);
    let out = aristo_in(ws.path())
        .args(["stamp"])
        .env("ARISTO_CANON_FIXTURE", &fixture_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stamp failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    ws
}

#[test]
fn suggestions_list_with_empty_queue_reports_none() {
    let ws = setup_workspace(SOURCE);
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();
    let out = aristo_in(ws.path())
        .args(["canon", "suggestions"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no proof-tree suggestions"),
        "got: {stdout}"
    );
}

#[test]
fn stamp_populates_queue_and_list_shows_the_cluster() {
    let ws = stamped_workspace();
    let out = aristo_in(ws.path())
        .args(["canon", "suggestions"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("proof-tree suggestions (1 cluster(s))"),
        "got: {stdout}"
    );
    assert!(stdout.contains("wal_protocol_correctness"), "got: {stdout}");
    assert!(stdout.contains("2 sibling(s)"), "got: {stdout}");
    assert!(
        stdout.contains("for wal_commit_requires_fsync"),
        "got: {stdout}"
    );
}

#[test]
fn suggestions_show_renders_objective_and_siblings() {
    let ws = stamped_workspace();
    let out = aristo_in(ws.path())
        .args(["canon", "suggestions", "wal_protocol_correctness"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("objective: wal_protocol_correctness"),
        "got: {stdout}"
    );
    assert!(stdout.contains("kanon: tier"), "got: {stdout}");
    assert!(stdout.contains("wal_find_frame_range_invariant"), "got: {stdout}");
    assert!(stdout.contains("wal_checkpoint_error_no_db_leak"), "got: {stdout}");
    // Subject-only: the card describes the user's invariants, never the
    // Lean model. The primary that dragged the cluster in is shown.
    assert!(
        stdout.contains("wal_commit_requires_fsync"),
        "got: {stdout}"
    );
}

#[test]
fn suggestions_show_unknown_cluster_errors() {
    let ws = stamped_workspace();
    let out = aristo_in(ws.path())
        .args(["canon", "suggestions", "does_not_exist"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no queued suggestion cluster"),
        "got: {stderr}"
    );
}

#[test]
fn suggestions_counts_emits_machine_readable_json() {
    let ws = stamped_workspace();
    let out = aristo_in(ws.path())
        .args(["canon", "suggestions", "--counts"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("not JSON: {e}\n{stdout}"));
    // Shape: {matches:{new,pending}, suggestions:{new,pending}}.
    assert!(v.get("matches").is_some(), "got: {v}");
    assert!(v.get("suggestions").is_some(), "got: {v}");
    assert!(v["matches"].get("new").is_some(), "got: {v}");
    assert!(v["matches"].get("pending").is_some(), "got: {v}");
    assert!(v["suggestions"].get("new").is_some(), "got: {v}");
    assert!(v["suggestions"].get("pending").is_some(), "got: {v}");
    // One cluster queued (still pending in the queue) → new=1.
    assert_eq!(v["suggestions"]["new"], 1, "got: {v}");
}

#[test]
fn dedup_two_drops_member_already_pending_in_cache() {
    // If a sibling is ALSO a primary match for some annotation (lands in
    // canon-matches.toml), dedup ② must keep it out of the suggestions
    // queue. We seed a second annotation whose text matches the sibling
    // canon_id via a tailored fixture: the second result returns the
    // sibling as a primary.
    let ws = setup_workspace(
        r#"
#[aristo::intent(
    "a commit frame must reach stable storage via fsync before durable",
    id = "wal_commit_durable_invariant"
)]
pub fn commit() {}

#[aristo::intent(
    "find_frame never reads outside the live frame range",
    id = "wal_find_frame_invariant"
)]
pub fn find_frame() {}
"#,
    );
    let fixture_dir = ws.path().join("fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    // Two annotations → two results. The SECOND primary IS the sibling
    // `wal_find_frame_range_invariant`; the suggestions cluster (aligned
    // to annotation 0) lists it as a sibling. Dedup ② must drop it.
    let body = r#"
effective_scopes = ["turso", ":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-05T00:00:00Z"

results = [
    [
        { canon_id = "wal_commit_requires_fsync", version = "v0.1.0", canonical_text = "a commit frame must reach stable storage via fsync", confidence = 0.91, scope = "turso", prefix_tier = "aristos:", backed_by = "golden model", linked = "arta_5b1c9f2a7e3d4068", verification = { coverage_level = "tight", test_binaries = [] } }
    ],
    [
        { canon_id = "wal_find_frame_range_invariant", version = "v0.1.0", canonical_text = "find_frame never reads outside the live frame range", confidence = 0.88, scope = "turso", prefix_tier = "aristos:", backed_by = "golden model", linked = "arta_aaaa1111bbbb2222", verification = { coverage_level = "tight", test_binaries = [] } }
    ]
]

[[suggestions]]
for_canon_id = "wal_commit_requires_fsync"
objective = { canon_id = "wal_protocol_correctness", version = "v0.1.0", canonical_text = "the WAL subsystem maintains correctness", scope = "turso", prefix_tier = "kanon:", backed_by = "", relationship = "parent" }
siblings = [
    { canon_id = "wal_find_frame_range_invariant", version = "v0.1.0", canonical_text = "find_frame never reads outside the live frame range", scope = "turso", prefix_tier = "aristos:", backed_by = "golden model", relationship = "sibling" },
    { canon_id = "wal_checkpoint_error_no_db_leak", version = "v0.1.0", canonical_text = "a checkpoint failure must not leak frames", scope = "turso", prefix_tier = "aristos:", backed_by = "golden model", relationship = "sibling" }
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();

    let out = aristo_in(ws.path())
        .args(["stamp"])
        .env("ARISTO_CANON_FIXTURE", &fixture_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let show = aristo_in(ws.path())
        .args(["canon", "suggestions", "wal_protocol_correctness"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    // The sibling that is also a primary match was dropped by dedup ②.
    assert!(
        !stdout.contains("wal_find_frame_range_invariant"),
        "dedup ② should drop the sibling that is a primary match; got: {stdout}"
    );
    // The other sibling survives.
    assert!(
        stdout.contains("wal_checkpoint_error_no_db_leak"),
        "got: {stdout}"
    );
}
