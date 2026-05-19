//! Slice 29 commit 4 — `aristo graph --format=svg`.
//!
//! Two integration tests covering the two paths the SVG emitter has
//! to handle:
//! - happy path: `dot` on PATH → produces a valid SVG file
//! - missing-binary path: `dot` not on PATH → exits 2 with the
//!   friendly install + alternatives message
//!
//! The corresponding `_pending/graph_format_svg.md` +
//! `graph_format_svg_no_dot.md` scenarios stay in `_pending/`:
//! - The happy-path SVG bytes differ across Graphviz versions
//!   (macOS brew vs Debian), so a byte-exact trycmd match would
//!   break cross-platform.
//! - The no-dot path requires faking `dot`'s absence, which trycmd
//!   has no surface for. Imperative tests can manipulate PATH; trycmd
//!   can't.
//!
//! Intent pinned (the spec): SVG fallback message must list the three
//! platforms AND the two no-Graphviz alternatives.

use assert_cmd::Command;
use std::path::Path;

fn aristo_in(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(repo);
    cmd
}

fn make_fixture_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::create_dir_all(repo.join(".aristo")).unwrap();
    std::fs::write(
        repo.join("aristo.toml"),
        "[project]\ncrate_root = \"src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".aristo/index.toml"),
        r#"[__meta__]
schema_version = 1

[demo_intent]
kind = "intent"
text = "demo"
verify = "neural"
status = "unknown"
text_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
body_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
file = "src/lib.rs"
site = "fn demo (line 1)"
covered_region = "function"
"#,
    )
    .unwrap();
    tmp
}

fn dot_on_path() -> bool {
    std::process::Command::new("dot")
        .arg("-V")
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn svg_renders_valid_xml_when_dot_is_on_path() {
    if !dot_on_path() {
        eprintln!("skipping: `dot` not on PATH (install graphviz to exercise this test)");
        return;
    }
    let tmp = make_fixture_workspace();
    let repo = tmp.path();
    let out_path = repo.join("out.svg");

    aristo_in(repo)
        .args(["graph", "--format=svg", "--include-orphans", "--out"])
        .arg(&out_path)
        .assert()
        .success();

    let svg = std::fs::read_to_string(&out_path).unwrap();
    // Verify Graphviz produced something SVG-shaped without pinning
    // version-dependent bytes.
    assert!(
        svg.starts_with("<?xml") || svg.contains("<svg "),
        "expected SVG content, got: {svg:.200}..."
    );
    assert!(svg.contains("demo_intent"), "node id should be in the SVG");
}

#[test]
fn svg_with_no_dot_on_path_errors_with_install_hints_and_alternatives() {
    let tmp = make_fixture_workspace();
    let repo = tmp.path();

    // Strip dot from PATH by setting PATH to a single empty dir. The
    // assert_cmd / std::process::Command will then fail to find `dot`
    // exactly the same way a user without graphviz would.
    let empty_path_dir = tmp.path().join("__no_dot_path__");
    std::fs::create_dir(&empty_path_dir).unwrap();

    let assert = Command::cargo_bin("aristo")
        .unwrap()
        .current_dir(repo)
        .env("PATH", &empty_path_dir)
        .args(["graph", "--format=svg", "--out=out.svg"])
        .assert()
        .failure()
        .code(2);

    // Friendly error message structure pinned by the unpromoted
    // _pending/graph_format_svg_no_dot.md scenario.
    let stderr_assertion = assert
        .stderr(predicates::str::contains(
            "SVG output requires Graphviz `dot`, which was not found on PATH",
        ))
        .stderr(predicates::str::contains("brew install graphviz"))
        .stderr(predicates::str::contains("apt install graphviz"))
        .stderr(predicates::str::contains("https://graphviz.org/download/"))
        .stderr(predicates::str::contains("--format=dot"))
        .stderr(predicates::str::contains("--format=mermaid"));
    let _ = stderr_assertion;

    // No file should have been written when the rendering fails.
    assert!(!repo.join("out.svg").exists());
}
