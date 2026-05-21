//! End-to-end scenario tests for `aristo critique --apply-findings`
//! with canonicalize-finding synthesis from `.aristo/canon-matches.toml`.
//! Maps to cli-sessions.md Flow 3 (review the match in a critique
//! session) — the apply-findings summary now surfaces canonicalize
//! findings alongside the agentic-critique findings per README L4.

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
    std::fs::create_dir_all(tmp.path().join(".aristo/critiques")).unwrap();
    std::fs::write(tmp.path().join("src").join("lib.rs"), source_body).unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname=\"sandbox\"\nversion=\"0.0.1\"\nedition=\"2021\"\n",
    )
    .unwrap();
    tmp
}

fn write_canon_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4", verification = { coverage_level = "tight", test_binaries = [] } }
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

#[test]
fn apply_findings_surfaces_canonicalize_from_canon_matches() {
    // Stamp populates canon-matches.toml with one pending aristos:-tier match.
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_canon_fixture(&fixture);
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stamp failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(ws.path().join(".aristo/canon-matches.toml").exists());

    // Now run apply-findings — should surface the canonicalize finding
    // even though there are no .critique files.
    let out = aristo_in(ws.path())
        .args(["critique", "--apply-findings"])
        .output()
        .expect("run critique");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("canonicalize: 1 open"),
        "expected canonicalize header, got: {stdout}"
    );
    assert!(
        stdout.contains("edit_page_cell_write_invariant"),
        "expected annotation id, got: {stdout}"
    );
    assert!(
        stdout.contains("cell_written_exactly_once_per_page_edit v0.2.1"),
        "expected canon match, got: {stdout}"
    );
    assert!(
        stdout.contains("aristos: tier"),
        "expected tier label, got: {stdout}"
    );
    assert!(
        stdout.contains("backed by: specialized neural checker"),
        "expected backing line, got: {stdout}"
    );
}

#[test]
fn apply_findings_no_canon_matches_file_is_silent() {
    let ws = setup_workspace(SOURCE);
    let _ = aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .output()
        .unwrap();
    let out = aristo_in(ws.path())
        .args(["critique", "--apply-findings"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("canonicalize"),
        "should not print canonicalize section when no cache; stdout: {stdout}"
    );
}

#[test]
fn apply_findings_empty_pending_does_not_surface_canonicalize_section() {
    // Cache exists but no pending findings (zero-match fixture).
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    std::fs::create_dir_all(&fixture).unwrap();
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
    let _ = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .output()
        .unwrap();

    let out = aristo_in(ws.path())
        .args(["critique", "--apply-findings"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No pending matches → no canonicalize section emitted.
    assert!(
        !stdout.contains("canonicalize:"),
        "empty pending should not surface header; stdout: {stdout}"
    );
}
