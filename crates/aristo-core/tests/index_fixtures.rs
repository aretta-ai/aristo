//! Fixture-driven round-trip tests for `.aristo/index.toml`.
//!
//! Walks `tests/fixtures/index/{valid,invalid}/*.toml` and asserts:
//!
//! - **valid/**: the file parses, `IndexFile::validate` succeeds, and
//!   serializing it back to TOML and re-parsing yields an identical
//!   `IndexFile` (round-trip equivalence).
//! - **invalid/**: the file either fails to parse OR fails
//!   `IndexFile::validate`. (Single combined gate; the test name
//!   doesn't try to predict which gate catches each case, since several
//!   plausible refactors of the type system shift the boundary.)
//!
//! Adding a new fixture requires no test code changes — drop a `.toml`
//! file in the appropriate directory.

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::index::IndexFile;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/index");

fn fixture_paths(subdir: &str) -> Vec<PathBuf> {
    let dir = Path::new(FIXTURE_DIR).join(subdir);
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixture dir {dir:?}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    paths
}

/// Each valid fixture: parse → validate → serialize → re-parse → equal.
#[test]
fn all_valid_fixtures_round_trip() {
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    let paths = fixture_paths("valid");
    assert!(!paths.is_empty(), "no valid fixtures discovered");
    for path in &paths {
        if let Err(e) = check_valid(path) {
            failures.push((path.clone(), e));
        }
    }
    if !failures.is_empty() {
        let report: String = failures
            .iter()
            .map(|(p, e)| format!("  ✗ {}\n      {}", p.display(), e.replace('\n', "\n      ")))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} of {} valid fixtures failed:\n{}",
            failures.len(),
            paths.len(),
            report
        );
    }
}

/// Each invalid fixture: parse OR validate fails.
#[test]
fn all_invalid_fixtures_rejected() {
    let mut surprises: Vec<PathBuf> = Vec::new();
    let paths = fixture_paths("invalid");
    assert!(!paths.is_empty(), "no invalid fixtures discovered");
    for path in &paths {
        if check_invalid(path).is_err() {
            surprises.push(path.clone());
        }
    }
    if !surprises.is_empty() {
        let report: String = surprises
            .iter()
            .map(|p| {
                format!(
                    "  ✗ {} parsed AND validated cleanly (expected failure)",
                    p.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} of {} invalid fixtures unexpectedly accepted:\n{}",
            surprises.len(),
            paths.len(),
            report
        );
    }
}

fn check_valid(path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let parsed: IndexFile = toml::from_str(&raw).map_err(|e| format!("parse failed: {e}"))?;
    parsed
        .validate()
        .map_err(|e| format!("validate failed: {e}"))?;
    let serialized = toml::to_string(&parsed).map_err(|e| format!("serialize failed: {e}"))?;
    let reparsed: IndexFile = toml::from_str(&serialized).map_err(|e| {
        format!("re-parse of serialized output failed: {e}\nSerialized was:\n{serialized}")
    })?;
    if parsed != reparsed {
        return Err(format!(
            "round-trip mismatch (parsed != re-parsed):\n  original entries: {} keys\n  re-parsed entries: {} keys",
            parsed.entries.len(),
            reparsed.entries.len(),
        ));
    }
    Ok(())
}

/// Returns `Err(())` if the fixture was unexpectedly accepted (parse +
/// validate both succeeded). `Ok(())` means the fixture was rejected at
/// some stage, which is the desired outcome.
fn check_invalid(path: &Path) -> Result<(), ()> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Ok(()), // can't read = effectively rejected
    };
    let parsed: IndexFile = match toml::from_str(&raw) {
        Ok(p) => p,
        Err(_) => return Ok(()), // parse failed = rejected
    };
    if parsed.validate().is_err() {
        return Ok(()); // validate failed = rejected
    }
    Err(()) // both succeeded — surprise
}
