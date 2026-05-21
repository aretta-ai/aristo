//! End-to-end scenario tests for `aristo canon reject` — pending →
//! rejected with text_hash pin per cli-sessions.md Flow 7.
//!
//! Pattern mirrors `canon_accept_command.rs`. The key invariant under
//! test is the L5 rejection-survives-until-text-changes contract:
//! rejection pins `(canon_id, text_hash)`; future stamps suppress
//! matches re-surfacing for the same tuple; once the annotation text
//! changes, the rejection no longer applies and the canon API gets
//! re-evaluated.

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

fn write_fixture(fixture_dir: &Path) {
    std::fs::create_dir_all(fixture_dir).unwrap();
    let body = r#"
effective_scopes = [":vanilla"]
canon_version = "v0.2.0"
matched_at = "2026-06-15T09:14:22Z"

results = [
    [
        { canon_id = "some_unrelated_entry", version = "v0.1.2", canonical_text = "output sequence is monotonically non-decreasing", confidence = 0.66, scope = ":vanilla", prefix_tier = "aristos:", backed_by = "specialized neural checker", linked = "arta_x1y2z3a4b5c6", verification = { coverage_level = "loose", test_binaries = [] } }
    ]
]
"#;
    std::fs::write(fixture_dir.join("match.toml"), body).unwrap();
}

const SOURCE: &str = r#"
#[aristo::intent(
    "result must be strictly increasing",
    id = "my_local_invariant"
)]
pub fn check_sequence() {}
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
fn reject_moves_pending_to_rejected_with_text_hash_pin() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "reject",
            "my_local_invariant",
            "some_unrelated_entry",
            "--reason",
            "intentionally narrower than canon entry",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "reject failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    // Pending is gone, rejected exists.
    assert!(
        !cache.contains("[[my_local_invariant.pending_matches]]"),
        "pending should be empty after reject; got:\n{cache}"
    );
    assert!(
        cache.contains("[[my_local_invariant.rejected_matches]]"),
        "expected rejected_matches entry; got:\n{cache}"
    );
    // The rejection carries canon_id + the user's reason.
    assert!(
        cache.contains(r#"canon_id = "some_unrelated_entry""#),
        "got: {cache}"
    );
    assert!(
        cache.contains(r#"reason = "intentionally narrower than canon entry""#),
        "got: {cache}"
    );
    // text_hash field is present (the pin).
    assert!(cache.contains("text_hash ="), "got: {cache}");
}

#[test]
fn reject_without_reason_still_succeeds_and_omits_reason() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "reject",
            "my_local_invariant",
            "some_unrelated_entry",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "reject (no reason) failed");

    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    assert!(
        cache.contains("[[my_local_invariant.rejected_matches]]"),
        "got: {cache}"
    );
    // No `reason =` line should appear when the user omitted --reason.
    let rejected_section_idx = cache
        .find("[[my_local_invariant.rejected_matches]]")
        .unwrap();
    let after = &cache[rejected_section_idx..];
    // Take just this rejected entry by stopping at the next double-bracket
    // section.
    let next_section = after[1..].find("[[").map(|i| i + 1).unwrap_or(after.len());
    let entry = &after[..next_section];
    assert!(
        !entry.contains("reason"),
        "expected no reason field for no-reason reject; got entry:\n{entry}"
    );
}

#[test]
fn reject_pin_uses_index_text_hash() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    // Read the index's text_hash for this annotation.
    let index_raw = std::fs::read_to_string(ws.path().join(".aristo/index.toml")).unwrap();
    let needle = "[my_local_invariant]";
    let section_start = index_raw.find(needle).expect("annotation in index");
    let section = &index_raw[section_start..];
    let th_idx = section
        .find("text_hash = \"")
        .expect("text_hash in section");
    let after = &section[th_idx + "text_hash = \"".len()..];
    let end = after.find('"').unwrap();
    let expected_text_hash = &after[..end];

    aristo_in(ws.path())
        .args([
            "canon",
            "reject",
            "my_local_invariant",
            "some_unrelated_entry",
        ])
        .status()
        .unwrap();

    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    let pin_line = format!(r#"text_hash = "{expected_text_hash}""#);
    assert!(
        cache.contains(&pin_line),
        "expected rejection to pin against the index's text_hash {expected_text_hash}; cache:\n{cache}"
    );
}

#[test]
fn reject_then_restamp_suppresses_the_same_match() {
    // Flow 7 invariant: after reject, the next stamp does NOT re-surface
    // the same (canon_id, text_hash) tuple as a pending match.
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    aristo_in(ws.path())
        .args([
            "canon",
            "reject",
            "my_local_invariant",
            "some_unrelated_entry",
            "--reason",
            "too broad",
        ])
        .status()
        .unwrap();

    // Re-stamp with the same fixture; the canon API would return the
    // same match, but it should be suppressed because the (canon_id,
    // text_hash) tuple is rejected.
    let out = aristo_in(ws.path())
        .env("ARISTO_CANON_FIXTURE", &fixture)
        .args(["stamp", "--refresh-canon"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "second stamp failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cache = std::fs::read_to_string(ws.path().join(".aristo/canon-matches.toml")).unwrap();
    assert!(
        !cache.contains("[[my_local_invariant.pending_matches]]"),
        "rejected match should not re-surface after stamp; cache:\n{cache}"
    );
    // The rejected_matches entry is still there.
    assert!(
        cache.contains("[[my_local_invariant.rejected_matches]]"),
        "rejected_matches should survive a restamp; cache:\n{cache}"
    );
}

// ─── error paths ─────────────────────────────────────────────────────────

#[test]
fn reject_with_unknown_canon_id_errors() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args(["canon", "reject", "my_local_invariant", "wrong_canon_id"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no pending canon match"),
        "expected diagnostic; got: {stderr}"
    );
}

#[test]
fn reject_with_unknown_annotation_id_errors() {
    let ws = setup_workspace(SOURCE);
    let fixture = ws.path().join("fixtures/canon");
    write_fixture(&fixture);
    stamp(ws.path(), &fixture);

    let out = aristo_in(ws.path())
        .args([
            "canon",
            "reject",
            "no_such_annotation",
            "some_unrelated_entry",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no pending canon matches")
            || stderr.contains("not found")
            || stderr.contains("not in"),
        "expected diagnostic; got: {stderr}"
    );
}
