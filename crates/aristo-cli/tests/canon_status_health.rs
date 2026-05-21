//! End-to-end scenario tests for the canon-health block of
//! `aristo status` (PR #11). Offline-only — no API call from status.

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

fn write_match_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("match.toml"),
        r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"
results = [
    [
        { canon_id = "cell_written_exactly_once_per_page_edit", version = "v0.2.1", canonical_text = "edit_page writes each cell exactly once", confidence = 0.92, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_a1b2c3d4ef56", verification = { coverage_level = "tight", test_binaries = [] } }
    ]
]
"#,
    )
    .unwrap();
}

#[test]
fn status_shows_canon_block_with_disabled_when_config_opts_out() {
    let ws = setup_workspace(SOURCE);
    std::fs::write(ws.path().join("aristo.toml"), "[canon]\nenabled = false\n").unwrap();
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();

    let out = aristo_in(ws.path()).args(["status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Canon binding"), "got: {stdout}");
    assert!(
        stdout.contains("disabled"),
        "expected disabled status; got: {stdout}"
    );
}

#[test]
fn status_shows_no_token_when_unauthenticated() {
    let ws = setup_workspace(SOURCE);
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();

    let out = aristo_in(ws.path()).args(["status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Canon binding"), "got: {stdout}");
    assert!(
        stdout.contains("no token") || stdout.contains("free-tier"),
        "expected no-token state; got: {stdout}"
    );
    assert!(
        stdout.contains("Last fetched:      never"),
        "expected never-fetched; got: {stdout}"
    );
}

#[test]
fn status_shows_cache_stats_after_stamp_and_accept() {
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

    let out = aristo_in(ws.path()).args(["status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Canon binding"), "got: {stdout}");
    assert!(
        stdout.contains("Catalog version:   v0.2.0"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Accepted (bound):  1"),
        "expected accepted=1; got: {stdout}"
    );
    assert!(
        stdout.contains("Pending:           0"),
        "expected pending=0 after accept; got: {stdout}"
    );
}
