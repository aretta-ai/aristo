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

use aristo_core::canon_verify::report::{DifferentialReport, Finding, FrameRole};

const MINIMAL: &str = include_str!("fixtures/cr03.minimal.json");
const PASS: &str = include_str!("fixtures/cr03-pass.json");
const FULL: &str = include_str!("fixtures/cr03.full.json");

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
fn full_fixture_round_trips_byte_stably() {
    let (reserialized, original) = round_trip(FULL);
    assert_eq!(
        reserialized, original,
        "cr03.full.json must round-trip through DifferentialReport unchanged"
    );
}

#[test]
fn full_fixture_call_path_shape() {
    // The enriched (Slice 3a) report carries the 14-frame turso spine.
    let full: DifferentialReport = serde_json::from_str(FULL).unwrap();
    assert_eq!(
        full.scenario.call_path.len(),
        14,
        "cr_03 spine is 14 frames"
    );

    // Exactly one fault frame: the injected sync site at prepare_wal_finish,
    // joined to op_trace seq 3.
    let faults: Vec<_> = full
        .scenario
        .call_path
        .iter()
        .filter(|f| f.role == FrameRole::Fault)
        .collect();
    assert_eq!(faults.len(), 1, "exactly one fault frame");
    assert!(
        faults[0].label.ends_with("prepare_wal_finish"),
        "fault frame is prepare_wal_finish; got {:?}",
        faults[0].label
    );
    assert_eq!(faults[0].io_seq, Some(3));

    // Exactly one effect frame: the pre-fault header write, joined to seq 2.
    let effects: Vec<_> = full
        .scenario
        .call_path
        .iter()
        .filter(|f| f.role == FrameRole::Effect)
        .collect();
    assert_eq!(effects.len(), 1, "exactly one effect frame");
    assert!(
        effects[0].label.ends_with("begin_write_wal_header"),
        "effect frame is begin_write_wal_header; got {:?}",
        effects[0].label
    );
    assert_eq!(effects[0].io_seq, Some(2));

    // The minimal + pass reports carry no spine — call_path is additive.
    let minimal: DifferentialReport = serde_json::from_str(MINIMAL).unwrap();
    assert!(
        minimal.scenario.call_path.is_empty(),
        "minimal report has no call_path"
    );
    let pass: DifferentialReport = serde_json::from_str(PASS).unwrap();
    assert!(
        pass.scenario.call_path.is_empty(),
        "pass report has no call_path"
    );
}

#[test]
fn full_fixture_test_shape() {
    // The enriched (Slice 3a) report carries the "what turso ran" workload
    // — the two SQL statements, the second of which faulted on commit.
    let full: DifferentialReport = serde_json::from_str(FULL).unwrap();
    let shape = &full.scenario.test_shape;
    assert_eq!(shape.len(), 2, "cr_03 test_shape is the 2 SQL statements");

    assert!(
        !shape[0].faulted,
        "the PRAGMA step did not fault; got faulted={}",
        shape[0].faulted
    );
    assert_eq!(shape[0].label, "PRAGMA journal_mode=WAL");

    assert!(
        shape[1].faulted,
        "the CREATE TABLE step is the one that faulted on commit"
    );
    assert!(
        shape[1].label.starts_with("CREATE TABLE"),
        "second step is the CREATE TABLE; got {:?}",
        shape[1].label
    );

    // test_shape is optional/additive: absent in minimal + pass reports.
    let minimal: DifferentialReport = serde_json::from_str(MINIMAL).unwrap();
    assert!(
        minimal.scenario.test_shape.is_empty(),
        "minimal report has no test_shape"
    );
    let pass: DifferentialReport = serde_json::from_str(PASS).unwrap();
    assert!(
        pass.scenario.test_shape.is_empty(),
        "pass report has no test_shape"
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
