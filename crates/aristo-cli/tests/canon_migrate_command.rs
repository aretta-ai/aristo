//! End-to-end scenario tests for `aristo canon migrate` (PR #12).
//! The diagnostic surface reports per-binding version drift between
//! the cache and the canon API for each canon-bound annotation.

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

const SOURCE: &str = r#"
#[aristo::intent(
    "each cell should be written exactly once per page edit",
    id = "edit_page_cell_write_invariant"
)]
pub fn edit_page() {}
"#;

fn write_match_fixture(fixture_dir: &Path, version: &str) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = format!(
        r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"
results = [
    [
        {{ canon_id = "cell_written_exactly_once_per_page_edit", version = "{version}", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4ef56", verification = {{ coverage_level = "tight", test_binaries = [] }} }}
    ]
]
"#
    );
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

fn write_empty_results_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("match.toml"),
        r#"
effective_scopes = [":vanilla"]
canon_version = "v0.3.0"
matched_at = "2026-06-15T09:14:22Z"
results = [[]]
"#,
    )
    .unwrap();
}

fn stamp_then_accept(ws: &Path, fixture: &Path) {
    aristo_in(ws)
        .env("ARISTO_CANON_FIXTURE", fixture)
        .args(["stamp"])
        .status()
        .unwrap();
    aristo_in(ws)
        .args([
            "canon",
            "accept",
            "edit_page_cell_write_invariant",
            "cell_written_exactly_once_per_page_edit",
        ])
        .status()
        .unwrap();
}

#[test]
fn migrate_with_no_canon_bound_annotations_reports_nothing_to_migrate() {
    let ws = setup_workspace(SOURCE);
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();

    let out = aristo_in(ws.path())
        .args(["canon", "migrate"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no canon-bound annotations"),
        "expected no-bindings message; got: {stdout}"
    );
}

#[test]
fn migrate_with_same_version_reports_current() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture, "v0.2.1");
    stamp_then_accept(ws.path(), &fixture);

    // Same fixture → migrate sees the same canon_id + version.
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["canon", "migrate"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("current"),
        "expected 'current' status; got: {stdout}"
    );
    assert!(
        stdout.contains("totals:") && stdout.contains("1 current"),
        "expected totals; got: {stdout}"
    );
}

#[test]
fn migrate_with_newer_version_reports_patch_bump() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture, "v0.2.1");
    stamp_then_accept(ws.path(), &fixture);

    // Bump fixture to v0.2.2 (patch bump on the same canon_id).
    write_match_fixture(&fixture, "v0.2.2");

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["canon", "migrate"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "migrate failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("patch-bump"),
        "expected patch-bump; got: {stdout}"
    );
    assert!(
        stdout.contains("v0.2.1 → v0.2.2"),
        "expected version transition; got: {stdout}"
    );
    assert!(
        stdout.contains("aristo canon refresh"),
        "expected refresh hint; got: {stdout}"
    );
}

#[test]
fn migrate_with_retired_canon_id_reports_minor_bump() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture, "v0.2.1");
    stamp_then_accept(ws.path(), &fixture);

    // Replace fixture with empty results — canon_id retired.
    write_empty_results_fixture(&fixture);

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["canon", "migrate"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("minor-bump"),
        "expected minor-bump; got: {stdout}"
    );
    assert!(
        stdout.contains("aristo canon unbind"),
        "expected unbind hint; got: {stdout}"
    );
}
