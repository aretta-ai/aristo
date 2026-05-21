//! End-to-end scenario tests for `aristo stamp` with canon-match
//! integration. The first PR that wires the canon substrate (auth,
//! HTTP client, cache, types) into a user-facing command — so this
//! is also the first PR with binary-level scenario coverage of the
//! canon flow.
//!
//! ## Test strategy
//!
//! Each test:
//! 1. Builds a sandbox workspace (tempdir) with a minimal
//!    `aristo.toml`, an `src/` tree with a Rust file containing one
//!    annotated function, and a fixture directory with a canned
//!    `match.toml` response.
//! 2. Runs `aristo init` to bootstrap `.aristo/`.
//! 3. Sets `ARISTO_CANON_FIXTURE=<fixture-dir>` so the SDK uses
//!    `MockCanonClient` instead of trying to reach the real server.
//! 4. Spawns `aristo stamp` and asserts on stdout/stderr + on-disk
//!    state (`.aristo/canon-matches.toml`).
//!
//! Maps to cli-sessions.md:
//!
//! - Flow 1 (high-confidence match surfaced) → `stamp_surfaces_high_confidence_match`
//! - Flow 2 (free-tier nudge) → `stamp_free_tier_skips_canon_with_nudge`
//! - Flow 6 (graceful degradation) → `stamp_unreachable_server_retains_cache`
//! - Cache-hit short-circuit → `stamp_cache_hit_skips_api_call`
//! - `--skip-canon` opt-out → `stamp_skip_canon_flag_honored`
//! - `[canon] enabled = false` opt-out → `stamp_canon_disabled_in_config`
//! - `--refresh-canon` invalidates cache → `stamp_refresh_canon_invalidates`

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn aristo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aristo")
}

/// Build an isolated `Command` that runs `aristo` in a sandbox
/// workspace, with `HOME`/`XDG_CONFIG_HOME`/`ARETTA_TOKEN` cleared
/// (so the user's real credentials don't leak into the test) and
/// `cwd` set to `workspace`.
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

/// Bootstrap a sandbox workspace with one annotated source file.
/// Returns the workspace TempDir + the canon-matches.toml path.
fn setup_workspace(workspace_text: &str, source_body: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("aristo.toml"), workspace_text).unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".aristo")).unwrap();
    std::fs::write(tmp.path().join("src").join("lib.rs"), source_body).unwrap();
    // Drop a Cargo.toml so any walker that needs it is happy.
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        r#"[package]
name = "sandbox"
version = "0.0.1"
edition = "2021"
"#,
    )
    .unwrap();
    tmp
}

/// Write a `match.toml` fixture for `MockCanonClient` that returns
/// one high-confidence aristos:-tier match for one annotation.
fn write_match_fixture_one_match(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4", verification = { coverage_level = "tight", test_binaries = ["monotonicity_property"] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

/// Empty results — server saw the annotations but had no matches above threshold.
fn write_match_fixture_no_matches(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [[]]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

/// Minimal aristo.toml with [canon] enabled (default — Pro behavior).
const ARISTO_TOML_DEFAULT: &str = "";

/// aristo.toml that opts out of canon entirely (regulated buyer pattern).
const ARISTO_TOML_CANON_DISABLED: &str = "[canon]\nenabled = false\n";

/// Source body with one `#[aristo::intent]` annotation. The id is
/// explicit (rather than stamp-assigned `aret_*`) so it stays
/// stable across multiple stamp invocations in a test — un-id'd
/// annotations get a fresh random opaque id every stamp run,
/// which would break cache-hit + check-mode tests that compare
/// state across runs.
const SOURCE_WITH_ONE_INTENT: &str = r#"
#[aristo::intent(
    "each cell should be written exactly once per page edit",
    id = "edit_page_cell_write_invariant"
)]
pub fn edit_page() {}
"#;

// ─── Flow 1: high-confidence match surfaced ───────────────────────────────

#[test]
fn stamp_surfaces_high_confidence_match() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .expect("run aristo stamp");
    assert!(
        out.status.success(),
        "stamp failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Summary line indicates one finding.
    assert!(
        stdout.contains("canon-match: 1 new finding"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("canon v0.2.0"), "stdout: {stdout}");
    // Per-match line includes id, version, confidence, tier.
    assert!(
        stdout.contains("cell_written_exactly_once_per_page_edit v0.2.1"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("conf 0.92"), "stdout: {stdout}");
    assert!(stdout.contains("aristos: tier"), "stdout: {stdout}");
    assert!(
        stdout.contains("backed by: specialized neural checker"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("review with `aristo critique"),
        "stdout: {stdout}"
    );

    // Cache file written on disk.
    let cache_path = ws.path().join(".aristo/canon-matches.toml");
    assert!(cache_path.exists(), "expected canon-matches.toml on disk");
    let cache_body = std::fs::read_to_string(&cache_path).unwrap();
    assert!(cache_body.contains("cell_written_exactly_once_per_page_edit"));
    assert!(cache_body.contains("canon_version = \"v0.2.0\""));
    assert!(cache_body.contains("disposition = \"open\""));
}

// ─── Flow 2: free-tier nudge ───────────────────────────────────────────────

#[test]
fn stamp_free_tier_skips_canon_with_nudge() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    // NO ARISTO_CANON_FIXTURE → MockCanonClient::from_env returns None.
    // NO ARETTA_TOKEN, no credentials file → auth::resolve returns NoToken.
    // Runner builds NoopCanonClient with is_free_tier = true.
    let out = aristo_in(ws.path())
        .args(["stamp"])
        .output()
        .expect("run aristo stamp");
    assert!(
        out.status.success(),
        "stamp failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("canon-match: skipped (Pro feature)"),
        "expected free-tier nudge, got: {stdout}"
    );
    assert!(stdout.contains("aristo auth login"), "stdout: {stdout}");

    // No canon-matches.toml written (free tier never writes it).
    assert!(!ws.path().join(".aristo/canon-matches.toml").exists());
}

// ─── --skip-canon: per-invocation opt-out ─────────────────────────────────

#[test]
fn stamp_skip_canon_flag_honored() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    // Even with a fixture available, --skip-canon must suppress the API call.
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp", "--skip-canon"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("canon-match: skipped (`--skip-canon`)"),
        "stdout: {stdout}"
    );
    // The skip suppresses cache writes (no API call was made).
    assert!(!ws.path().join(".aristo/canon-matches.toml").exists());
}

// ─── [canon] enabled = false: project-level opt-out ───────────────────────

#[test]
fn stamp_canon_disabled_in_config() {
    let ws = setup_workspace(ARISTO_TOML_CANON_DISABLED, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("canon-match: skipped (disabled"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("[canon] enabled = false"),
        "stdout: {stdout}"
    );
}

// ─── Cache-hit short-circuit ──────────────────────────────────────────────

#[test]
fn stamp_cache_hit_skips_api_call() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    // First stamp: produces a canon-matches.toml with one pending match.
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Delete the fixture so a real API call would have nothing to read.
    std::fs::remove_file(fixture.join("match.toml")).unwrap();

    // Second stamp: should hit the cache (text_hash unchanged) and NOT
    // make an API call. The mock client would error out without the
    // fixture, so success here is the proof.
    let out2 = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(
        out2.status.success(),
        "second stamp should succeed via cache hit; stderr: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let stdout = String::from_utf8_lossy(&out2.stdout);
    assert!(
        stdout.contains("canon-match: cache hit"),
        "expected cache-hit summary, got: {stdout}"
    );
}

// ─── --refresh-canon: invalidate cache ────────────────────────────────────

#[test]
fn stamp_refresh_canon_invalidates_cache() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    // First stamp: populates cache.
    let _ = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    // Verify cache exists
    assert!(ws.path().join(".aristo/canon-matches.toml").exists());

    // Second stamp with --refresh-canon: re-queries even though text unchanged.
    // To prove we re-queried, delete the fixture and expect a Degraded
    // outcome (or test by swapping the fixture to "no matches").
    write_match_fixture_no_matches(&fixture);
    let out2 = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp", "--refresh-canon"])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let stdout = String::from_utf8_lossy(&out2.stdout);
    // Either "0 new finding" or "no annotations need a fresh match" —
    // since refresh forces re-query, we see a fresh API call returning
    // 0 matches.
    assert!(
        stdout.contains("canon-match: 0 new finding"),
        "expected refresh to re-query and find 0 matches, got: {stdout}"
    );
}

// ─── Flow 6: graceful degradation (API failure → cache retained) ─────────

#[test]
fn stamp_unreachable_server_retains_cache() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    // First stamp: populates cache.
    let _ = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    let cache_before = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml"))
        .expect("cache should exist after first stamp");

    // Drift the annotation text — forces a re-query on the next stamp.
    std::fs::write(
        ws.path().join("src/lib.rs"),
        r#"
#[aristo::intent("each cell is written exactly one time during a page edit")]
pub fn edit_page() {}
"#,
    )
    .unwrap();

    // Delete the fixture so the mock client returns CanonError::Fixture.
    // The SDK should treat this as a Degraded outcome and continue.
    std::fs::remove_file(fixture.join("match.toml")).unwrap();
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .expect("run stamp");
    assert!(
        out.status.success(),
        "graceful degradation should not fail stamp; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("canon-match: skipped"),
        "expected degraded-skip, got: {stdout}"
    );
    assert!(
        stdout.contains("cached matches retained"),
        "stdout: {stdout}"
    );

    // Cache file still exists (we never wrote over it on the failure).
    let cache_after = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml"))
        .expect("cache should be retained");
    assert_eq!(cache_before, cache_after, "cache must survive API failure");
}

// ─── --check mode skips canon (CI doesn't need outbound calls) ────────────

#[test]
fn stamp_check_mode_does_not_call_canon() {
    // --check should NOT touch canon (CI invariant). Even with a
    // fixture available and a fresh source file, --check should
    // return without writing canon-matches.toml.
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_one_match(&fixture);

    // Need to stamp once (non-check) to get an index that --check
    // can compare against; otherwise --check would exit non-zero
    // for the unrelated "index out of sync" reason.
    let _ = aristo_in(ws.path())
        .args(["stamp", "--skip-canon"]) // first stamp, no canon noise
        .output()
        .unwrap();
    // Wipe any canon-matches.toml from that first stamp.
    let _ = std::fs::remove_file(ws.path().join(".aristo/canon-matches.toml"));

    // Now --check with a fixture available. The canon step should
    // not run because we early-return before reaching it.
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp", "--check"])
        .output()
        .expect("run stamp --check");
    assert!(
        out.status.success(),
        "check mode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // No canon-matches.toml should have been written during --check.
    assert!(
        !ws.path().join(".aristo/canon-matches.toml").exists(),
        "--check must not write canon-matches.toml"
    );
}

// ─── No matches in response: not an error, just "0 findings" ──────────────

#[test]
fn stamp_zero_matches_is_success_with_zero_findings() {
    let ws = setup_workspace(ARISTO_TOML_DEFAULT, SOURCE_WITH_ONE_INTENT);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture_no_matches(&fixture);

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("canon-match: 0 new finding"),
        "stdout: {stdout}"
    );
    // Cache file still written (records the no-match state +
    // last_match_text_hash for the cache-skip on next run).
    let cache_path = ws.path().join(".aristo/canon-matches.toml");
    assert!(cache_path.exists());
}
