//! Phase 16 — the typed, relation-kind-polymorphic verify-result record.
//!
//! Mirrors the canonical machine-validated contract at
//! `docs/mockups/16-verify-failure-ux/contract/differential-report.schema.json`
//! byte-for-byte. Every repo (aristo SDK, aretta-books harness,
//! aretta-code proxy) conforms to this shape. The golden fixtures
//! (`cr03.minimal.json` / `cr03-pass.json`) are the cross-repo tether —
//! `tests/report_contract.rs` proves struct ⟺ contract round-trip
//! stability against byte-copies of them.
//!
//! ## Shape
//!
//! A [`DifferentialReport`] answers, in order:
//!
//! - [`Property`] — WHAT claim was checked (canon id + plain statement +
//!   the spec/impl source anchors that render the card's two rows).
//! - [`Scenario`] — under WHAT condition (optional injected [`FaultSpec`],
//!   the ordered [`OpEvent`] timeline, the determinism `seed`, and
//!   fuzz-only [`ShrinkStats`]).
//! - [`Verdict`] — the verdict frame (optional `cr_id`, optional
//!   [`ExpectedToFail`] when the failure IS the conformance verdict).
//! - [`Relation`] — what was compared vs. deliberately ignored (the mask).
//! - [`Finding`] — the relation-kind-polymorphic body, internally tagged
//!   on `kind`. [`Finding::StateEq`] ships first; add a variant per
//!   relation family as they land.
//!
//! Optional fields are absent (not `null`) when `None`, matching serde's
//! `skip_serializing_if` — `additionalProperties: false` in the schema
//! means an emitted `null` would fail validation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The typed verify-result record. Top-level container nesting the five
/// always-present sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DifferentialReport {
    pub property: Property,
    pub scenario: Scenario,
    pub verdict: Verdict,
    pub relation: Relation,
    pub finding: Finding,
}

/// WHAT claim was checked. `canon_id` + `statement` are the headline;
/// the two sources render the "spec" and "impl" rows of the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Property {
    /// Bare canon id (tier prefix lives on the annotation),
    /// e.g. `wal_initialized_reflects_sync_outcome`.
    pub canon_id: String,
    /// Plain-language invariant — the headline. From canon
    /// `canonical_text`.
    pub statement: String,
    /// Where the property is encoded as a conformance test (harness).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_source: Option<SourceRef>,
    /// The implicated production code anchor (from `index.toml`
    /// `source_path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impl_source: Option<SourceRef>,
}

/// A `<path>:<line>` anchor with an optional symbol/excerpt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub path: String,
    pub line: u32,
    /// Optional symbol/excerpt, e.g. the function name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Under WHAT condition. `fault` + `op_trace` are populated by R2/R3
/// (Slice 3); in the Slice-1 minimal report `fault` is absent and
/// `op_trace` is `[]`. `seed` is the deterministic repro key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Scenario {
    /// The named, parameterized injected fault (R2). Absent for
    /// fault-free scenarios.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<FaultSpec>,
    /// Ordered IO/SQL timeline (R3). Empty in the Slice-1 minimal report.
    #[serde(default)]
    pub op_trace: Vec<OpEvent>,
    /// The turso call-path spine (Slice 3a): entry → … → fault/effect,
    /// IP-filtered to `turso_core` frames. A sibling of `op_trace` (not a
    /// field on [`OpEvent`]) because the spine forks across two ops — the
    /// pre-fault effect write and the fault — and so cannot hang off any
    /// single one. Empty/absent in minimal + op-trace-only reports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_path: Vec<CallFrame>,
    /// The top-level calls the test made into the subject library — the
    /// card's "what turso ran" view. For cr_03, the SQL statements. Labels
    /// are the harness's human-readable per-call descriptors (in production
    /// an XRay custom-event payload injected at the harness->subject
    /// boundary, NOT parsed from trace symbols). Optional/additive: empty
    /// when no workload was captured.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_shape: Vec<TestStep>,
    /// Determinism token / repro key. For fixed scenarios a content-hash
    /// of (test, fault, op-script).
    pub seed: String,
    /// Fuzz-only shrink stats. Absent for fixed scenarios.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shrink: Option<ShrinkStats>,
}

/// The named, parameterized injected fault (R2 — `Policy::describe()`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FaultSpec {
    /// e.g. `fail_nth`.
    pub kind: String,
    /// e.g. `Sync`.
    pub op: String,
    pub nth: u64,
    /// e.g. `EIO`.
    pub error: String,
    /// How many times it actually fired.
    pub fired_count: u64,
}

/// One ordered entry in the IO/SQL timeline (R3 — `RecordingHook`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpEvent {
    /// 1-indexed.
    pub seq: u64,
    /// `pwrite | sync | PRAGMA | SQL ...`.
    pub op: String,
    /// `@0 len=32 | journal_mode=WAL | the SQL`.
    pub detail: String,
    /// `true` on the event the fault hit.
    pub injected: bool,
    /// `ok | EIO | Ok | Err(..)`.
    pub result: String,
}

/// One top-level call the test made into the subject library — the
/// "what turso ran" / test-shape view. `label` is the harness's
/// human-readable descriptor of the call (the SQL for a SQL subject, or
/// any op-descriptor for another subject — opaque + generic; the
/// production source is an XRay custom-event payload injected at the
/// harness→subject boundary, NOT parsed from the trace symbols).
/// `faulted` marks the call during which the divergence/fault arose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TestStep {
    /// Human-readable description of the subject call, e.g. the SQL text.
    pub label: String,
    /// `true` on the call during which the fault / divergence occurred.
    /// Always serialized (the golden always provides it); defaults to
    /// `false` on the wire for older/absent reports.
    #[serde(default)]
    pub faulted: bool,
}

/// One frame on the turso call-path spine (Slice 3a) — the card's
/// "where your code broke it" view. Machine-extracted from the XRay
/// trace, normalized + IP-filtered to `turso_core` frames.
///
/// `depth` carries the tree shape: the spine is a pre-order walk, and a
/// `depth` that is `<=` a prior frame's `depth` begins a new branch off
/// the shared prefix (e.g. cr_03 forks at `commit_dirty_pages_inner` into
/// the effect write and the fault sync). `role` switches the renderer's
/// inline marker; `io_seq` back-refs the [`OpEvent`] this frame's
/// bridge-IO call-site produced (the join key into `op_trace`).
///
/// `file`/`line` are absent until the capture pipeline symbolizes
/// (Slice 3c) — meanwhile the precise anchor for the headline frame lives
/// on `property.impl_source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CallFrame {
    /// Normalized turso fn symbol (generics/wrappers stripped), e.g.
    /// `turso_core::storage::wal::WalFile::prepare_wal_finish`. The
    /// contract carries the final string; the renderer shortens for
    /// display.
    pub label: String,
    /// Position in the pruned semantic spine, root = 0. A pre-order
    /// sequence; a value `<=` a prior frame's `depth` begins a new branch
    /// off the shared prefix.
    pub depth: u32,
    /// Defining-crate-relative path, e.g. `core/storage/wal.rs`. Also the
    /// IP filter key. Populated by the symbolizer (Slice 3c); absent in
    /// the symbol-only spine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Paired with `file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// `normal` | `fault` (the injected fault / panic site) | `effect`
    /// (the divergent-field producer, possibly a pre-fault sibling). Off
    /// the wire — never re-derived in the renderer.
    pub role: FrameRole,
    /// Back-ref to [`OpEvent::seq`] when this frame is a bridge-IO
    /// call-site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_seq: Option<u64>,
}

/// The role of a [`CallFrame`] on the spine — switches the renderer's
/// inline marker. Comes off the wire; never re-derived in the renderer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameRole {
    /// An ordinary spine frame.
    #[default]
    Normal,
    /// The injected fault / panic site.
    Fault,
    /// The divergent-field producer (possibly a pre-fault sibling).
    Effect,
}

/// Fuzz-only (proptest-style). `None`/absent for fixed scenarios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShrinkStats {
    pub from: u64,
    pub to: u64,
    pub steps: u64,
}

/// The verdict frame. `cr_id` present when the divergence maps to a
/// known CR; `expected_to_fail` present when the failure IS the
/// conformance verdict (its reason is the unblock condition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Verdict {
    /// e.g. `CR-03`. `None` when not CR-mapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cr_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_to_fail: Option<ExpectedToFail>,
}

/// The "expected to fail" frame: the failure is the verdict and its
/// `reason` is the unblock condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExpectedToFail {
    pub tag: String,
    /// The unblock condition — what would make this go green.
    pub reason: String,
}

/// What was compared and what was deliberately ignored (the mask).
/// `kind` is the clean machine discriminant (== finding's `kind`, both
/// from `Equivalence::variant_tag()`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    /// `state_eq | predicate_eq | refines | ...` — the discriminant.
    pub kind: String,
    /// Human form, e.g. `StateEq under ProjectionMask { initialized }`.
    pub description: String,
    pub compared: Vec<String>,
    pub ignored: Vec<String>,
}

/// Relation-kind-polymorphic body, internally tagged on `kind`.
/// [`Finding::StateEq`] ships first; add a variant per relation family
/// touched (predicate/refines/snapshot_eq/monotone/member_of/subset_of/
/// behavioral/sweep/invariant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Finding {
    /// State-equality finding: both projected snapshots + the minimal
    /// masked delta.
    StateEq {
        /// Model / reference (oracle) side.
        expected: Snapshot,
        /// Implementation (system under test) side.
        actual: Snapshot,
        /// Only the fields that diverge, after the mask. The minimal
        /// counterexample.
        divergence: Vec<FieldDivergence>,
    },
}

/// A labelled projection of named fields (one side of a `StateEq`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    /// e.g. `lean (reference)` / `turso (under test)`.
    pub label: String,
    pub fields: Vec<NamedField>,
}

/// One named field within a [`Snapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NamedField {
    pub field: String,
    /// Stringified (Debug) value.
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// One diverging field after the mask — the minimal counterexample row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldDivergence {
    pub field: String,
    /// Model value (stringified).
    pub expected: String,
    /// Impl value (stringified).
    pub actual: String,
    /// Why the divergence exists — the proxy/durability collapse story.
    /// Renders as "why this matters".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}
