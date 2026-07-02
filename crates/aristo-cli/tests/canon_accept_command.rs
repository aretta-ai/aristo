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
//! 9. `accept_carries_instrumentation_bundle_into_accepted_match`
//!    (P-008 SLICE23-SPEC: verification metadata + bundle survive
//!    match → pending → accepted through the real binary)
//! 10. `accept_tie_break_prefers_aristos_tier_on_equal_confidence`
//!     (dual-tier same-canon-id rows, equal confidence → aristos: wins)

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

// ─── sibling-shift regression ────────────────────────────────────────────

/// Two multi-line `#[aristo::intent(...)]` annotations in the same
/// file. Accepting the first rewrites it to a one-liner, shifting
/// the second annotation's line. Without the sibling-site refresh,
/// the second accept fails with "no attribute found at line N"
/// because the index still records the second annotation's pre-shift
/// line.
const TWO_MULTILINE_SOURCE: &str = r#"
#[aristo::intent(
    "each cell should be written exactly once per page edit",
    id = "first_anno"
)]
pub fn first_fn() {}

#[aristo::intent(
    "total can't be negative",
    id = "second_anno"
)]
pub fn second_fn() {}
"#;

fn write_two_match_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4ef56", verification = { coverage_level = "tight", test_binaries = [] } }
    ],
    [
        { canon_id = "checkout_total_non_negative", version = "v0.1.0", canonical_text = "checkout total is non-negative", confidence = 0.94, scope = ":vanilla", prefix_tier = "kanon:", linked = "arta_b2c3d4e5f6a7", verification = { coverage_level = "loose", test_binaries = [] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

#[test]
fn accept_then_sibling_accept_succeeds_after_line_shift() {
    let ws = setup_workspace(TWO_MULTILINE_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_two_match_fixture(&fixture);
    stamp(ws.path(), &fixture);

    // First accept: rewrites first_fn's multi-line attribute to a
    // one-liner. This collapses ~3 source lines → 1, shifting
    // second_fn upward in the file.
    let out1 = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "first_anno",
            "cell_written_exactly_once_per_page_edit",
        ])
        .output()
        .unwrap();
    assert!(
        out1.status.success(),
        "first accept failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out1.stdout),
        String::from_utf8_lossy(&out1.stderr)
    );

    // Second accept on the sibling. Without the sibling-site refresh,
    // this fails because the index still records second_fn's
    // pre-shift line. The fix re-walks the file after the first
    // rewrite and updates all annotations' `site` fields with their
    // current (post-shift) lines.
    let out2 = aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "second_anno",
            "checkout_total_non_negative",
        ])
        .output()
        .unwrap();
    assert!(
        out2.status.success(),
        "second accept failed (sibling-shift regression): stdout={} stderr={}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );

    // Both rewrites landed in source.
    let post = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(
        post.contains(r#"id = "aristos:cell_written_exactly_once_per_page_edit""#),
        "expected first annotation's aristos: prefix in source; got:\n{post}"
    );
    assert!(
        post.contains(r#"id = "kanon:checkout_total_non_negative""#),
        "expected second annotation's kanon: prefix in source; got:\n{post}"
    );
}

// ─── equal-confidence dual-tier tie-break regression ─────────────────────

/// Server response listing the SAME canon entry at BOTH prefix tiers
/// (same `canon_id`/`version`, equal confidence), with the `kanon:`
/// row FIRST — the server's order.
fn write_dual_tier_tie_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "kanon:", linked = "arta_a1b2c3d4ef56", verification = { coverage_level = "loose", test_binaries = [] } },
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4ef56", verification = { coverage_level = "tight", test_binaries = [] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

/// Regression: when the same canon entry is pending at both prefix
/// tiers with equal confidence, accept must tie-break to the
/// `aristos:` row. It used to sort by confidence only and take the
/// server's first row (`kanon:`), writing a `kanon:`-prefixed id
/// into SUT source — which aristo-macros 0.3 rejects as an invalid
/// identifier, breaking the SUT build on 0.3-pinned forks.
#[test]
fn accept_tie_break_prefers_aristos_tier_on_equal_confidence() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_dual_tier_tie_fixture(&fixture);
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

    // The rewritten source must carry the aristos: prefix — never
    // kanon:, despite the kanon row being listed first by the server.
    let post = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(
        post.contains(r#"id = "aristos:cell_written_exactly_once_per_page_edit""#),
        "expected aristos: prefix in source on equal-confidence tie; got:\n{post}"
    );
    assert!(
        !post.contains("kanon:"),
        "kanon: prefix must not reach source (0.3 macro rejects it); got:\n{post}"
    );

    // Index is re-keyed under the aristos:-prefixed id too.
    let index_raw = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    assert!(
        index_raw.contains(r#"["aristos:cell_written_exactly_once_per_page_edit"]"#),
        "expected aristos:-prefixed entry in index; got:\n{index_raw}"
    );
    assert!(
        !index_raw.contains("kanon:cell_written_exactly_once_per_page_edit"),
        "index must not carry the kanon:-prefixed id; got:\n{index_raw}"
    );
}

// ─── P-008 instrumentation-bundle carry (SLICE23-SPEC aristo item 2) ──────

/// Build a match fixture whose verification block carries a P-008
/// instrumentation bundle. Constructed with the real wire types and
/// serialized with `toml::to_string` (the mock client reads TOML) —
/// hand-writing the nested bundle as inline-table TOML would be
/// unreadable and typo-prone.
fn write_instrumented_fixture(fixture_dir: &Path) {
    use aristo_core::canon::{
        BundleCompanion, BundleCompileCheck, BundleProvenance, CanonMatch, CanonMatchResponse,
        InstrumentationBundle, InstrumentationRecord, PrefixTier, RecordLanding, RecordPresence,
        VerificationMetadata,
    };
    let mut sut_binding = std::collections::BTreeMap::new();
    sut_binding.insert("turso_core".to_string(), "core".to_string());
    let bundle = InstrumentationBundle {
        bundle_id: "turso:7b6cbae:ae85f8792372".into(),
        provenance: BundleProvenance {
            base_ref: "ad351877c5cf38c1fafc7f08703bfe521b8f4437".into(),
            payload_ref: "7b6cbaec04e86c0d9ac47819c77444af5054c50a".into(),
            macro_grammar_rev: "aristo-macros 0.3.0 (two-mode Inspect grammar)".into(),
            sut_binding,
            authored_at: "7b6cbaec04e86c0d9ac47819c77444af5054c50a".into(),
        },
        compile_check: BundleCompileCheck {
            package: "turso_core".into(),
            features: "aristo-instr,turso_core/aristo-instr".into(),
        },
        companions: vec![BundleCompanion {
            symbol: "WalInstalledSnapshot".into(),
            role: "return_type".into(),
            file: "core/types.rs".into(),
            visibility: "pub (cfg aristo-instr)".into(),
            payload_ref: Some("7b6cbaec".into()),
        }],
        records: vec![
            InstrumentationRecord {
                accessor_id: "inspect_header_version".into(),
                kind: "inspect_projection".into(),
                class: "A".into(),
                semantic_tier: "none".into(),
                intent: "Expose the in-memory logical-log header version.".into(),
                catch: "Logical-log durability catch (durability).".into(),
                landing: RecordLanding {
                    target: serde_json::json!({
                        "crate": "turso_core",
                        "container": "LogicalLog",
                        "field": "header"
                    }),
                    annotation: Some(
                        "#[cfg_attr(feature = \"aristo-instr\", inspect(name = \"header_version\"))]"
                            .into(),
                    ),
                    ensure_derive: Some(
                        "#[cfg_attr(feature = \"aristo-instr\", derive(Inspect))]".into(),
                    ),
                    required_use: vec![],
                    companions_ref: vec![],
                },
                presence: RecordPresence {
                    expected_symbol: "LogicalLog::inspect_header_version".into(),
                    expected_signature: "fn inspect_header_version(&self) -> Option<u8>".into(),
                    harness_probe: Some(
                        "let _r: Option<u8> = log.inspect_header_version();".into(),
                    ),
                },
                oracle: None,
                upstream_status: "local-only".into(),
            },
            // Mirrors the golden fixture's record 2: hand-written
            // accessor with all-None optional fields, so the
            // end-to-end carry pins that Nones survive the real
            // binary's TOML writes as absent keys → None.
            InstrumentationRecord {
                accessor_id: "installed_snapshot".into(),
                kind: "hand_written_fn".into(),
                class: "A".into(),
                semantic_tier: "required".into(),
                intent: "Owned snapshot of the installed read-snapshot fields.".into(),
                catch: "WAL install coherence.".into(),
                landing: RecordLanding {
                    target: serde_json::json!({
                        "crate": "turso_core",
                        "container": "Wal (trait) / impl Wal for WalFile",
                        "method": "installed_snapshot"
                    }),
                    annotation: None,
                    ensure_derive: None,
                    required_use: vec![],
                    companions_ref: vec!["WalInstalledSnapshot".into()],
                },
                presence: RecordPresence {
                    expected_symbol: "Wal::installed_snapshot".into(),
                    expected_signature: "fn installed_snapshot(&self) -> WalInstalledSnapshot"
                        .into(),
                    harness_probe: None,
                },
                oracle: None,
                upstream_status: "local-only".into(),
            },
        ],
    };
    let resp = CanonMatchResponse {
        results: vec![vec![CanonMatch {
            canon_id: "cell_written_exactly_once_per_page_edit".into(),
            version: "v0.2.1".into(),
            canonical_text: "edit_page writes each cell exactly once".into(),
            confidence: 0.92,
            scope: ":vanilla".into(),
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("specialized neural checker".into()),
            linked: Some("arta_a1b2c3d4ef56".into()),
            verification: VerificationMetadata {
                coverage_level: "tight".into(),
                test_binaries: vec!["wal_install_coherence".into()],
                instrumentation: Some(bundle),
            },
        }]],
        effective_scopes: vec![":vanilla".into()],
        canon_version: "v0.2.0".into(),
        matched_at: "2026-07-02T09:14:22Z".into(),
        suggestions: None,
    };
    std::fs::create_dir_all(fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("match.toml"),
        toml::to_string(&resp).unwrap(),
    )
    .unwrap();
}

/// THE carry round-trip (SLICE23-SPEC aristo item 2): a match result
/// carrying an instrumentation bundle → `stamp` persists it on the
/// PendingMatch → `canon accept` moves it onto the AcceptedMatch —
/// all through the real binary and the on-disk
/// `.aristo/canon-matches.toml`.
#[test]
fn accept_carries_instrumentation_bundle_into_accepted_match() {
    use aristo_core::canon::CanonMatchesFile;
    use aristo_core::index::AnnotationId;

    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_instrumented_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let cache_path = ws.path().join(".aristo/canon-matches.toml");

    // Post-stamp: the pending match carries the verification block.
    let pre = CanonMatchesFile::read(&cache_path).unwrap();
    let ann_id = AnnotationId::parse("edit_page_cell_write_invariant").unwrap();
    let pending = &pre.entries[&ann_id].pending_matches[0];
    let vm = pending
        .verification
        .as_ref()
        .expect("stamp must persist verification metadata on the pending match");
    assert_eq!(vm.coverage_level, "tight");
    assert_eq!(vm.test_binaries, vec!["wal_install_coherence"]);
    let pending_bundle = vm
        .instrumentation
        .as_ref()
        .expect("stamp must persist the instrumentation bundle");
    assert_eq!(pending_bundle.bundle_id, "turso:7b6cbae:ae85f8792372");
    assert_eq!(pending_bundle.records.len(), 2);

    // Accept moves it to accepted_matches under the prefixed id.
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

    let post = CanonMatchesFile::read(&cache_path).unwrap();
    let prefixed = AnnotationId::parse("aristos:cell_written_exactly_once_per_page_edit").unwrap();
    let accepted = &post.entries[&prefixed].accepted_matches[0];
    let accepted_vm = accepted
        .verification
        .as_ref()
        .expect("accept must carry verification metadata onto the accepted match");
    assert_eq!(accepted_vm.coverage_level, "tight");
    assert_eq!(accepted_vm.test_binaries, vec!["wal_install_coherence"]);
    let bundle = accepted_vm
        .instrumentation
        .as_ref()
        .expect("accept must carry the instrumentation bundle");
    // Bundle content survives the whole match → pending → accepted
    // lifecycle byte-equal.
    assert_eq!(bundle, pending_bundle);
    assert_eq!(bundle.records[0].accessor_id, "inspect_header_version");
    assert!(bundle.records[0].landing.annotation.is_some());
    let hand_written = &bundle.records[1];
    assert_eq!(hand_written.accessor_id, "installed_snapshot");
    assert!(
        hand_written.landing.annotation.is_none(),
        "None optionals must survive the on-disk TOML round-trip as None"
    );
    assert!(hand_written.presence.harness_probe.is_none());
    assert!(hand_written.oracle.is_none());
    assert_eq!(
        hand_written.landing.companions_ref,
        vec!["WalInstalledSnapshot"]
    );
    assert_eq!(bundle.companions.len(), 1);

    // The union helper sees exactly this bundle from the cache.
    let union = aristo_core::canon::union_accepted_bundles(&post);
    assert!(union.warnings.is_empty(), "got: {:?}", union.warnings);
    assert_eq!(union.bundles.len(), 1);
    assert_eq!(&union.bundles[0], bundle);
}

#[test]
fn binding_survives_a_subsequent_stamp_run() {
    // The original snag: `aristo stamp` (run e.g. by the pre-commit
    // hook after `git commit`) regenerates `.aristo/index.toml` from
    // source, and the walker can't know the server-issued `linked`
    // ref. Before the derive-from-cache fix, stamp emitted every
    // entry as BindingState::Local, wiping the binding the previous
    // `aristo canon accept` had written to the index — and breaking
    // `aristo verify --tags`, which requires Bound.
    //
    // After the fix, stamp reads the canon-matches cache and
    // re-derives BindingState::Bound for any entry whose id carries
    // a canon prefix and has a matching `accepted_matches` row. This
    // test pins that behavior end-to-end.
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp(ws.path(), &fixture);

    // Accept binds the entry: index has BindingState::Bound { linked }.
    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    let post_accept = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    assert!(
        post_accept.contains(r#"linked = "arta_a1b2c3d4ef56""#),
        "post-accept must have linked set; got:\n{post_accept}"
    );

    // The provocation: stamp again. The walker discovers the same
    // source (now with the prefixed `id = "aristos:..."`), build_entries
    // emits BindingState::Local, and the derive-from-cache step must
    // restore BindingState::Bound from the cache's accepted_matches row.
    stamp(ws.path(), &fixture);

    let post_stamp = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    assert!(
        post_stamp.contains(r#"["aristos:cell_written_exactly_once_per_page_edit"]"#),
        "post-stamp must still have the prefixed entry; got:\n{post_stamp}"
    );
    assert!(
        post_stamp.contains(r#"linked = "arta_a1b2c3d4ef56""#),
        "post-stamp must preserve linked via derive-from-cache; got:\n{post_stamp}"
    );
}
