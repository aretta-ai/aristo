//! End-to-end scenario tests for the `aristo canon {list, show,
//! refresh}` subcommands (PR #8). Pattern mirrors the other canon_*
//! command e2e tests.

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

fn write_match_fixture(fixture_dir: &Path) {
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

const SOURCE: &str = r#"
#[aristo::intent(
    "each cell should be written exactly once per page edit",
    id = "edit_page_cell_write_invariant"
)]
pub fn edit_page() {}
"#;

// ─── canon list ──────────────────────────────────────────────────────────

#[test]
fn list_with_empty_cache_reports_no_matches() {
    let ws = setup_workspace(SOURCE);
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();
    let out = aristo_in(ws.path())
        .args(["canon", "list"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no canon matches"),
        "expected empty-cache message; got: {stdout}"
    );
}

#[test]
fn list_after_stamp_shows_pending_match() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture);
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = aristo_in(ws.path())
        .args(["canon", "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("edit_page_cell_write_invariant"),
        "expected annotation id; got: {stdout}"
    );
    assert!(
        stdout.contains("[pending]") && stdout.contains("cell_written_exactly_once"),
        "expected pending match line; got: {stdout}"
    );
    assert!(
        stdout.contains("aristo canon accept edit_page_cell_write_invariant"),
        "expected a ready-to-run accept command under the pending match; got: {stdout}"
    );
    assert!(
        stdout.contains("totals:") && stdout.contains("1 pending"),
        "expected totals; got: {stdout}"
    );
}

#[test]
fn list_after_accept_shows_accepted_match() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture);
    aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .status()
        .unwrap();
    aristo_in(ws.path())
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();

    let out = aristo_in(ws.path())
        .args(["canon", "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[accepted]"),
        "expected accepted bucket in listing; got: {stdout}"
    );
    assert!(
        stdout.contains("aristos:cell_written_exactly_once_per_page_edit"),
        "expected prefixed key; got: {stdout}"
    );
}

// ─── canon show (coming soon) ─────────────────────────────────────────────

#[test]
fn show_reports_coming_soon() {
    // `canon show` is deferred (coming soon) pending the per-instance
    // data-plane rework: it makes no canon-API call and no longer
    // depends on auth or a fixture — any invocation reports the
    // NotImplemented deferral (exit 64).
    let ws = setup_workspace(SOURCE);
    let out = aristo_in(ws.path())
        .args(["canon", "show", "anything"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "coming-soon command must exit non-zero"
    );
    assert_eq!(out.status.code(), Some(64), "NotImplemented exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("canon show") && stderr.contains("not implemented yet"),
        "expected coming-soon diagnostic; got: {stderr}"
    );
}

// ─── canon refresh ───────────────────────────────────────────────────────

#[test]
fn refresh_re_runs_the_match_call_against_index() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture);
    aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .status()
        .unwrap();

    // Swap fixture to zero matches, run refresh; cache should reflect.
    std::fs::write(
        fixture.join("match.toml"),
        r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"
results = [[]]
"#,
    )
    .unwrap();

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["canon", "refresh"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "refresh failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    assert!(
        !cache.contains("[[edit_page_cell_write_invariant.pending_matches]]"),
        "refresh should have cleared the pending match; got cache:\n{cache}"
    );
}
