//! Fixture-driven round-trip tests for `aristo.toml`.
//!
//! Mirrors `tests/index_fixtures.rs` and `tests/spec_fixtures.rs`. Every
//! valid fixture parses, re-serializes, re-parses to an equal
//! `ConfigFile`. Every invalid fixture fails to parse.

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::config::ConfigFile;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/config");

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
            "{} of {} valid config fixtures failed:\n{}",
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
            "{} of {} invalid config fixtures unexpectedly accepted:\n{}",
            surprises.len(),
            paths.len(),
            report
        );
    }
}

fn check_valid(path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let parsed: ConfigFile = toml::from_str(&raw).map_err(|e| format!("parse failed: {e}"))?;
    let serialized = toml::to_string(&parsed).map_err(|e| format!("serialize failed: {e}"))?;
    let reparsed: ConfigFile = toml::from_str(&serialized).map_err(|e| {
        format!("re-parse of serialized output failed: {e}\nSerialized was:\n{serialized}")
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
    let result: Result<ConfigFile, _> = toml::from_str(&raw);
    if result.is_err() {
        return Ok(());
    }
    Err(())
}
