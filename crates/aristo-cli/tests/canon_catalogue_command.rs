//! End-to-end test for `aristo canon catalogue` — downloads the canon
//! catalogue to a gitignored `.aristo/catalogue.json` snapshot via a
//! fixture, and refuses when logged out.

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

fn setup_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("aristo.toml"), "").unwrap();
    std::fs::create_dir_all(tmp.path().join(".aristo")).unwrap();
    tmp
}

fn write_catalogue_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
[[entries]]
canon_id = "foo"
version = "v0.2.1"
canonical_text = "edit_page writes each cell exactly once"
category = "invariants"
applies_to = ["fn"]
coverage_level = "tight"
spec_refs = ["S-001"]
backed_by = { ":vanilla" = "specialized neural checker" }

[[entries]]
canon_id = "bar"
version = "v0.1.0"
canonical_text = "sequence numbers stay monotonic"
category = "concurrency"
applies_to = ["fn"]
coverage_level = "none"
spec_refs = []
backed_by = {}
"#;
    std::fs::write(fixture_dir.join("catalogue.toml"), body).unwrap();
}

#[test]
fn catalogue_downloads_snapshot_to_gitignored_file() {
    let ws = setup_workspace();
    let fixture = ws.path().join("fixtures/canon");
    write_catalogue_fixture(&fixture);

    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["canon", "catalogue"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "catalogue failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("downloaded 2 canon entries"),
        "expected download summary; got: {stdout}"
    );
    assert!(
        stdout.contains(".aristo/catalogue.json"),
        "expected the snapshot path; got: {stdout}"
    );

    // The snapshot file was written and holds both entries + backing.
    let snapshot = std::fs::read_to_string(ws.path().join(".aristo/catalogue.json")).unwrap();
    assert!(snapshot.contains("foo"), "snapshot: {snapshot}");
    assert!(snapshot.contains("bar"), "snapshot: {snapshot}");
    assert!(
        snapshot.contains("specialized neural checker"),
        "snapshot: {snapshot}"
    );
}

#[test]
fn catalogue_without_auth_refuses() {
    let ws = setup_workspace();
    // No ARISTO_CANON_FIXTURE, no token: the command must refuse rather
    // than silently no-op.
    let out = aristo_in(ws.path())
        .args(["canon", "catalogue"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("authentication") || stderr.contains("login"),
        "expected auth diagnostic; got: {stderr}"
    );
}
