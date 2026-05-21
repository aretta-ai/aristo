//! End-to-end scenario tests for the trust-card extension of
//! `aristo show <bound_id>` (PR #10). When the queried annotation has
//! a canon-prefixed id (`aristos:...` / `kanon:...`), the renderer
//! appends a trust-card block sourced from local state.

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

fn write_kanon_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("match.toml"),
        r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"
results = [
    [
        { canon_id = "checkout_total_non_negative", version = "v0.1.0", canonical_text = "checkout total is non-negative", confidence = 0.94, scope = ":vanilla", prefix_tier = "kanon:", linked = "arta_b2c3d4e5f6a7", verification = { coverage_level = "loose", test_binaries = [] } }
    ]
]
"#,
    )
    .unwrap();
}

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

fn stamp_then_accept(ws: &Path, fixture: &Path, ann: &str, canon: &str) {
    aristo_in(ws)
        .env("ARISTO_CANON_FIXTURE", fixture)
        .args(["stamp"])
        .status()
        .unwrap();
    aristo_in(ws)
        .args(["canon", "accept", ann, canon])
        .status()
        .unwrap();
}

#[test]
fn show_on_aristos_bound_id_renders_backed_trust_card() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_aristos_fixture(&fixture);
    stamp_then_accept(
        ws.path(),
        &fixture,
        "edit_page_cell_write_invariant",
        "cell_written_exactly_once_per_page_edit",
    );

    let out = aristo_in(ws.path())
        .args(["show", "aristos:cell_written_exactly_once_per_page_edit"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "show failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Canon binding"),
        "expected trust card header; got: {stdout}"
    );
    assert!(
        stdout.contains("aristos:"),
        "expected aristos: tier label; got: {stdout}"
    );
    assert!(
        stdout.contains("specialized neural checker"),
        "expected backed_by; got: {stdout}"
    );
    assert!(
        stdout.contains("aristo canon show"),
        "expected canon-show hint; got: {stdout}"
    );
    // Heavy box-drawing rule should appear (aristos: tier).
    assert!(
        stdout.contains("═"),
        "expected heavy box rule for aristos:; got: {stdout}"
    );
}

#[test]
fn show_on_kanon_bound_id_renders_unbacked_trust_card() {
    let ws = setup_workspace(KANON_SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_kanon_fixture(&fixture);
    stamp_then_accept(
        ws.path(),
        &fixture,
        "checkout_total_invariant",
        "checkout_total_non_negative",
    );

    let out = aristo_in(ws.path())
        .args(["show", "kanon:checkout_total_non_negative"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Canon binding"), "got: {stdout}");
    assert!(stdout.contains("kanon:"), "got: {stdout}");
    assert!(
        stdout.contains("no verification backing"),
        "expected no-backing message; got: {stdout}"
    );
    assert!(
        stdout.contains("aristo canon request-verify checkout_total_non_negative"),
        "expected request-verify hint with bare canon id; got: {stdout}"
    );
    // Light box-drawing rule should appear (kanon: tier).
    assert!(
        stdout.contains("─"),
        "expected light box rule for kanon:; got: {stdout}"
    );
}

#[test]
fn show_on_local_id_does_not_render_trust_card() {
    let ws = setup_workspace(ARISTOS_SOURCE);
    aristo_in(ws.path())
        .args(["stamp", "--skip-canon"])
        .status()
        .unwrap();

    let out = aristo_in(ws.path())
        .args(["show", "edit_page_cell_write_invariant"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("Canon binding"),
        "local id should not show trust card; got: {stdout}"
    );
}
