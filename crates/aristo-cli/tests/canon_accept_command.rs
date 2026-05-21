//! End-to-end scenario tests for `aristo canon accept` — the source
//! rewrite + index rebind + cache pending → accepted move per
//! cli-sessions.md Flow 4 (`aristos:` tier) and Flow 5 (`kanon:` tier).
//!
//! Pattern mirrors `canon_stamp_command.rs`: spawn the real `aristo`
//! binary in a sandbox workspace, drive the canon match via
//! `ARISTO_CANON_FIXTURE` for stamp (populating `pending_matches`),
//! then run `aristo canon accept <ann_id> <canon_id>` and assert on
//! the on-disk state.
//!
//! Scenarios covered:
//!
//! 1. `accept_aristos_tier_rewrites_source_and_applies_prefix` (Flow 4)
//! 2. `accept_kanon_tier_rewrites_source_and_applies_kanon_prefix` (Flow 5)
//! 3. `accept_updates_index_binding_state_to_bound`
//! 4. `accept_moves_pending_to_accepted_in_cache`
//! 5. `accept_replaces_text_with_canonical_in_index`
//! 6. `accept_with_unknown_annotation_id_errors`
//! 7. `accept_with_unknown_canon_id_errors`
//! 8. `accept_already_bound_annotation_refuses`

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

fn write_aristos_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4ef56", verification = { coverage_level = "tight", test_binaries = [] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

fn write_kanon_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "checkout_total_non_negative", version = "v0.1.0", canonical_text = "checkout total is non-negative", confidence = 0.94, scope = ":vanilla", prefix_tier = "kanon:", linked = "arta_b2c3d4e5f6a7", verification = { coverage_level = "loose", test_binaries = [] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

/// Source with one intent annotation carrying a pinned `id =` so the
/// id stays stable across stamp runs (stamp would otherwise assign a
/// random `aret_*` id, breaking string assertions).
const ARISTOS_SOURCE: &str = r#"
#[aristo::intent(
    "each cell should be written exactly once per page edit",
    id = "edit_page_cell_write_invariant"
)]
pub fn edit_page() {}
"#;

const KANON_SOURCE: &str = r#"
#[aristo::intent(
    "total can't be negative",
    id = "checkout_total_invariant"
)]
pub fn compute_total() {}
"#;

fn stamp(ws: &Path, fixture: &Path) {
    let out = aristo_in(ws)
        .env("ARISTO_CANON_FIXTURE", fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stamp failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ─── happy paths ─────────────────────────────────────────────────────────

#[test]
fn accept_aristos_tier_rewrites_source_and_applies_prefix() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "accept failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let post = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(
        post.contains(r#"id = "aristos:cell_written_exactly_once_per_page_edit""#),
        "expected aristos: prefix in source; got:\n{post}"
    );
    assert!(
        post.contains(r#""edit_page writes each cell exactly once""#),
        "expected canonical text in source; got:\n{post}"
    );
    // Original prose is gone.
    assert!(
        !post.contains("should be written exactly once per page edit"),
        "expected original prose to be replaced; got:\n{post}"
    );
}

#[test]
fn accept_kanon_tier_rewrites_source_and_applies_kanon_prefix() {
    let ws = setup_workspace(KANON_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_kanon_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "checkout_total_invariant",
            "checkout_total_non_negative",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "accept failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let post = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(
        post.contains(r#"id = "kanon:checkout_total_non_negative""#),
        "expected kanon: prefix; got:\n{post}"
    );
    assert!(!post.contains("aristos:"), "post: {post}");
    assert!(
        post.contains(r#""checkout total is non-negative""#),
        "expected canonical text; got:\n{post}"
    );
}

#[test]
fn accept_updates_index_binding_state_to_bound() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    let index_raw = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    // The entry is re-keyed under the canon-prefixed id.
    assert!(
        index_raw.contains(r#"["aristos:cell_written_exactly_once_per_page_edit"]"#),
        "expected canon-prefixed entry in index; got:\n{index_raw}"
    );
    // The `linked` field appears (proving BindingState::Bound).
    assert!(
        index_raw.contains(r#"linked = "arta_a1b2c3d4ef56""#),
        "expected linked field set in index; got:\n{index_raw}"
    );
    // The old id is gone.
    assert!(
        !index_raw.contains("[edit_page_cell_write_invariant]"),
        "old id should be removed from index; got:\n{index_raw}"
    );
}

#[test]
fn accept_moves_pending_to_accepted_in_cache() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let cache_path = ws.path().join(".aristo/canon-matches.toml");
    let pre = std::fs::read_to_string(&cache_path).unwrap();
    // Pre-accept: pending exists under bare id, no accepted entries.
    assert!(
        pre.contains("[[edit_page_cell_write_invariant.pending_matches]]"),
        "pre-accept must have pending under bare id; got:\n{pre}"
    );

    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    let post = std::fs::read_to_string(&cache_path).unwrap();
    // Post-accept: pending is gone under bare id; accepted appears
    // under the prefixed id.
    assert!(
        !post.contains("[[edit_page_cell_write_invariant.pending_matches]]"),
        "post-accept must have no pending under bare id; got:\n{post}"
    );
    assert!(
        post.contains(r#"[["aristos:cell_written_exactly_once_per_page_edit".accepted_matches]]"#),
        "post-accept must have accepted under prefixed id; got:\n{post}"
    );
    // The accepted entry carries canonical_text + canon_id + version.
    assert!(
        post.contains(r#"canon_id = "cell_written_exactly_once_per_page_edit""#),
        "post: {post}"
    );
    assert!(post.contains(r#"version = "v0.2.1""#), "post: {post}");
}

#[test]
fn accept_replaces_text_with_canonical_in_index() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    let index_raw = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    // Index `text` should now be the canonical phrasing.
    assert!(
        index_raw.contains("text = \"edit_page writes each cell exactly once\""),
        "expected canonical text in index; got:\n{index_raw}"
    );
}

// ─── error paths ─────────────────────────────────────────────────────────

#[test]
fn accept_with_unknown_annotation_id_errors() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "no_such_annotation",
            "cell_written_exactly_once_per_page_edit",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "should error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no pending canon matches")
            || stderr.contains("not found")
            || stderr.contains("no such"),
        "expected unknown-id diagnostic; got: {stderr}"
    );
}

#[test]
fn accept_with_unknown_canon_id_errors() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "some_other_canon_id",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "should error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no pending canon match"),
        "expected unknown-canon-id diagnostic; got: {stderr}"
    );
}

#[test]
fn accept_already_bound_annotation_refuses() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    // First accept: succeeds.
    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    // Second accept: the source now carries `aristos:` prefix, so
    // looking up the bare `edit_page_cell_write_invariant` finds no
    // entry (it's now `aristos:cell_written_exactly_once_per_page_edit`).
    // The refusal surfaces via "no pending canon matches" — the cache
    // entry was moved. This is the right shape: re-running on a
    // bound annotation is a no-op error, not a double-apply.
    let out = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "re-accept must refuse");
}
