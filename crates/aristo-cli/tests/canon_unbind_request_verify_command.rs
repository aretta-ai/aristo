//! End-to-end scenario tests for `aristo canon unbind` and
//! `aristo canon request-verify` (PR #9).

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

// ─── canon unbind ────────────────────────────────────────────────────────

#[test]
fn unbind_after_accept_reverts_source_and_index() {
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

    // Source now carries aristos:cell_written... — confirm.
    let after_accept = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(after_accept.contains("aristos:cell_written"));

    // Unbind.
    let out = aristo_in(ws.path())
        .args([
            "canon",
            "unbind",
            "aristos:cell_written_exactly_once_per_page_edit",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "unbind failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Source: aristos: prefix gone; bare canon_id in its place.
    let post = std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap();
    assert!(
        !post.contains("aristos:"),
        "post-unbind source must not contain aristos:; got:\n{post}"
    );
    assert!(
        post.contains(r#"id = "cell_written_exactly_once_per_page_edit""#),
        "expected bare id; got:\n{post}"
    );

    // Index: prefixed key gone, bare key present, binding Local.
    let index = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    assert!(
        !index.contains("aristos:cell_written"),
        "post-unbind index must not contain aristos: key; got:\n{index}"
    );
    assert!(
        index.contains("[cell_written_exactly_once_per_page_edit]"),
        "expected bare key in index; got:\n{index}"
    );
    assert!(
        !index.contains("linked = \"arta_"),
        "binding should be Local (no linked field); got:\n{index}"
    );

    // Cache: accepted_matches entry under prefixed id is gone.
    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    assert!(
        !cache.contains("aristos:cell_written"),
        "post-unbind cache must not contain aristos: key; got:\n{cache}"
    );
}

#[test]
fn unbind_non_canon_bound_id_errors() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_match_fixture(&fixture);
    aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp"])
        .status()
        .unwrap();

    // Try to unbind a bare id (not prefixed).
    let out = aristo_in(ws.path())
        .args(["canon", "unbind", "edit_page_cell_write_invariant"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not canon-bound") || stderr.contains("aristos:"),
        "expected non-canon-bound diagnostic; got: {stderr}"
    );
}

// ─── canon request-verify (coming soon) ──────────────────────────────────

#[test]
fn request_verify_reports_coming_soon() {
    // `canon request-verify` is deferred (coming soon) pending the
    // per-instance data-plane rework: it makes no canon-API call and no
    // longer depends on auth or a fixture — any invocation reports the
    // NotImplemented deferral (exit 64).
    let ws = setup_workspace(SOURCE);
    let out = aristo_in(ws.path())
        .args(["canon", "request-verify", "anything"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "coming-soon command must exit non-zero"
    );
    assert_eq!(out.status.code(), Some(64), "NotImplemented exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("canon request-verify") && stderr.contains("not implemented yet"),
        "expected coming-soon diagnostic; got: {stderr}"
    );
}
