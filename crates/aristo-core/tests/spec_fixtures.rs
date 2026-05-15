//! Fixture-driven round-trip tests for `.aristo/specs/<id>.spec`.
//!
//! Mirrors `tests/index_fixtures.rs`: walks
//! `tests/fixtures/spec/{valid,invalid}/*.spec` and asserts that valid
//! files parse, re-serialize via `Display`, and re-parse to an equal
//! `SpecFile`; invalid files fail at parse time. Adding a new fixture
//! requires no test code changes.

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::spec::SpecFile;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/spec");

fn fixture_paths(subdir: &str) -> Vec<PathBuf> {
    let dir = Path::new(FIXTURE_DIR).join(subdir);
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixture dir {dir:?}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("spec"))
        .collect();
    paths.sort();
    paths
}

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
            "{} of {} valid spec fixtures failed:\n{}",
            failures.len(),
            paths.len(),
            report
        );
    }
}

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
            .map(|p| format!("  ✗ {} parsed cleanly (expected failure)", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{} of {} invalid spec fixtures unexpectedly accepted:\n{}",
            surprises.len(),
            paths.len(),
            report
        );
    }
}

fn check_valid(path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let parsed = SpecFile::parse(&raw).map_err(|e| format!("parse failed: {e}"))?;
    let rendered = parsed.to_string();
    let reparsed = SpecFile::parse(&rendered).map_err(|e| {
        format!("re-parse of rendered output failed: {e}\nRendered was:\n{rendered}")
    })?;
    if parsed != reparsed {
        return Err("round-trip mismatch (parsed != re-parsed)".into());
    }
    Ok(())
}

fn check_invalid(path: &Path) -> Result<(), ()> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    if SpecFile::parse(&raw).is_err() {
        return Ok(());
    }
    Err(())
}
