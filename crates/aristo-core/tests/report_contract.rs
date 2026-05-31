//! Phase 16 Track A — `DifferentialReport` contract conformance.
//!
//! The canonical schema + golden fixtures live in the sibling
//! meta-workspace (`docs/mockups/16-verify-failure-ux/contract/`). The
//! fixtures here are byte-copies of that cross-repo tether; the Rust
//! types MUST serialize/deserialize that exact JSON shape.
//!
//! For each golden fixture we prove struct ⟺ contract round-trip
//! stability: parse the file → into `DifferentialReport` → back to a
//! `serde_json::Value`, and assert it equals the original file parsed
//! as a `Value`. Any field drift (rename, casing, skip, extra field)
//! breaks this — that's the point.

use aristo_core::canon_verify::report::{DifferentialReport, Finding};

const MINIMAL: &str = include_str!("fixtures/cr03.minimal.json");
const PASS: &str = include_str!("fixtures/cr03-pass.json");

/// Round-trip helper: JSON text → `DifferentialReport` → `Value`, and
/// the original text → `Value`. Returns (reserialized, original) for
/// equality assertion.
fn round_trip(json: &str) -> (serde_json::Value, serde_json::Value) {
    let report: DifferentialReport =
        serde_json::from_str(json).expect("fixture must deserialize into DifferentialReport");
    let reserialized = serde_json::to_value(&report).expect("report must serialize to Value");
    let original: serde_json::Value =
        serde_json::from_str(json).expect("fixture must parse as raw Value");
    (reserialized, original)
}

#[test]
fn minimal_fixture_round_trips_byte_stably() {
    let (reserialized, original) = round_trip(MINIMAL);
    assert_eq!(
        reserialized, original,
        "cr03.minimal.json must round-trip through DifferentialReport unchanged"
    );
}

#[test]
fn pass_fixture_round_trips_byte_stably() {
    let (reserialized, original) = round_trip(PASS);
    assert_eq!(
        reserialized, original,
        "cr03-pass.json must round-trip through DifferentialReport unchanged"
    );
}

#[test]
fn minimal_fixture_key_fields() {
    let report: DifferentialReport = serde_json::from_str(MINIMAL).unwrap();

    assert_eq!(
        report.property.canon_id,
        "wal_initialized_reflects_sync_outcome"
    );
    assert_eq!(report.relation.kind, "state_eq");
    assert_eq!(report.relation.compared, vec!["initialized".to_string()]);
    assert_eq!(report.verdict.cr_id.as_deref(), Some("CR-03"));

    // The internally-tagged Finding::StateEq must carry exactly one
    // divergence: `initialized` false → true.
    let Finding::StateEq { divergence, .. } = &report.finding;
    assert_eq!(
        divergence.len(),
        1,
        "minimal fixture has one diverging field"
    );
    assert_eq!(divergence[0].field, "initialized");
    assert_eq!(divergence[0].expected, "false");
    assert_eq!(divergence[0].actual, "true");
    assert!(divergence[0].provenance.is_some());

    // expected_to_fail unblock reason mentions the atomic accessor.
    let etf = report
        .verdict
        .expected_to_fail
        .as_ref()
        .expect("minimal fixture carries expected_to_fail");
    assert!(etf.reason.contains("wal_initialized_atomic"));
}

#[test]
fn pass_fixture_has_empty_divergence_and_no_expected_to_fail() {
    let report: DifferentialReport = serde_json::from_str(PASS).unwrap();
    let Finding::StateEq { divergence, .. } = &report.finding;
    assert!(divergence.is_empty(), "pass fixture has no divergence");
    assert!(report.verdict.expected_to_fail.is_none());
    assert_eq!(report.verdict.cr_id.as_deref(), Some("CR-03"));
}
