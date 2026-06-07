//! `aristo-core::metrics` — the canonical, index-derived metrics surface
//! (Phase 18 #9, the nudge/progress engine's COMPUTE leg).
//!
//! [`Metrics`] is the single source of truth for the annotation counts that
//! `aristo status`, `aristo badge`, `aristo metrics`, and the nudge engine
//! all report. Producing them from ONE counting pass ([`Metrics::from_index`]
//! plus the shared [`compute_tier`] formula) is what keeps those surfaces
//! from drifting on the same underlying numbers (audit A2: converge the
//! *counting pass*, not the published scalar — each surface still derives
//! its own headline number from these shared counts).
//!
//! This core type holds only what is derivable from the index alone. The
//! cli layers the runtime signals it can't see from here — queue depth,
//! canon-match counts, the reviewed/proof-reviewed maps, the edit-window
//! baseline, the prior-score snapshot — in its union function (S0c+).

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::badge::{compute_tier, Tier};
use crate::index::{IndexEntry, IndexFile, VerifyLevel, VerifyMethod};

/// Schema version for the `aristo metrics --json` payload. Bump on any
/// breaking change to [`Metrics`]'s shape so consumers can gate.
pub const METRICS_SCHEMA_VERSION: u32 = 1;

/// Index-derived project metrics — counts, the unverified backlog, and the
/// tier/score. Serializes to the `aristo metrics --json` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Metrics {
    /// Payload schema version (see [`METRICS_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Total `intent` annotations (every `verify` level, including `false`).
    pub intents: usize,
    /// Total `assume` annotations — external invariants, never verified.
    pub assumes: usize,
    /// Intents with `verify != false`: the verifiable surface (the
    /// denominator of [`Metrics::verification_rate`] and the unverified
    /// backlog).
    pub verifiable: usize,
    /// Verifiable intents currently in a terminal-clean status
    /// (`Status::is_terminal_clean`: Verified / Tested / Neural).
    pub verified_clean: usize,
    /// `verifiable - verified_clean`: the unverified backlog the
    /// `verify_backlog` nudge signal watches.
    pub unverified: usize,
    /// `verified_clean / verifiable`, or `0.0` when nothing is verifiable.
    /// This is the engine's own rate; it is intentionally NOT the badge's
    /// `verification_rate_pct` (which divides by ALL intents) — both derive
    /// from the same shared counts (A2).
    pub verification_rate: f64,
    /// The project tier (Aspirant … ✦ Areté), via the depth-weighted badge
    /// formula.
    pub tier: Tier,
    /// The `[0, 1]` visible score backing the tier.
    pub visible_score: f64,
}

#[aristo::intent(
    "The verifiable surface is intents with `verify != false` only — assumes \
     are external invariants and never verified, and `verify = false` intents \
     are documentation-only. `verified_clean` counts the verifiable intents in \
     a terminal-clean status (the shared `Status::is_terminal_clean`), and \
     `unverified` is exactly `verifiable - verified_clean`, so the three never \
     disagree. The rate divides by `verifiable` (not by all intents), and is \
     0 when nothing is verifiable rather than a divide-by-zero.",
    verify = "test",
    id = "metrics_verifiable_excludes_assumes_and_doc_only_intents"
)]
impl Metrics {
    /// Compute every index-derived metric in one pass. `fn_counts` is the
    /// per-module function surface the coverage score needs (the cli walks
    /// source for it); `default_method` resolves `verify = true` for the
    /// tier formula (from `[verify] default_method`).
    pub fn from_index(
        index: &IndexFile,
        fn_counts: &BTreeMap<PathBuf, u32>,
        default_method: Option<VerifyMethod>,
    ) -> Self {
        let mut intents = 0usize;
        let mut assumes = 0usize;
        let mut verifiable = 0usize;
        let mut verified_clean = 0usize;

        for entry in index.entries.values() {
            match entry {
                IndexEntry::Intent(e) => {
                    intents += 1;
                    // verify=false intents are documentation-only: never
                    // verifiable, so excluded from the backlog and rate.
                    if !matches!(e.verify, VerifyLevel::Bool(false)) {
                        verifiable += 1;
                        if e.status.is_terminal_clean() {
                            verified_clean += 1;
                        }
                    }
                }
                IndexEntry::Assume(_) => assumes += 1,
            }
        }

        let unverified = verifiable - verified_clean;
        let verification_rate = if verifiable == 0 {
            0.0
        } else {
            verified_clean as f64 / verifiable as f64
        };

        let computation = compute_tier(index, fn_counts, default_method);

        Metrics {
            schema_version: METRICS_SCHEMA_VERSION,
            intents,
            assumes,
            verifiable,
            verified_clean,
            unverified,
            verification_rate,
            tier: computation.tier,
            visible_score: computation.visible_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{
        AnnotationId, AssumeEntry, BindingState, CoveredRegion, IndexEntry, IndexFile, IntentEntry,
        Meta, Sha256, Status, VerifyLevel, VerifyMethod,
    };

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(verify: VerifyLevel, status: Status) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn f".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn assume() -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: "y".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn g".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn index_of(entries: Vec<IndexEntry>) -> IndexFile {
        let mut map = std::collections::BTreeMap::new();
        for (i, e) in entries.into_iter().enumerate() {
            map.insert(AnnotationId::parse(&format!("id_{i}")).unwrap(), e);
        }
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: map,
        }
    }

    fn metrics_of(entries: Vec<IndexEntry>) -> Metrics {
        Metrics::from_index(&index_of(entries), &BTreeMap::new(), None)
    }

    #[test]
    fn empty_index_is_all_zero_aspirant() {
        let m = metrics_of(vec![]);
        assert_eq!(m.intents, 0);
        assert_eq!(m.assumes, 0);
        assert_eq!(m.verifiable, 0);
        assert_eq!(m.verified_clean, 0);
        assert_eq!(m.unverified, 0);
        assert_eq!(m.verification_rate, 0.0);
        assert_eq!(m.tier, Tier::Aspirant);
        assert_eq!(m.visible_score, 0.0);
        assert_eq!(m.schema_version, METRICS_SCHEMA_VERSION);
    }

    #[test]
    fn assumes_are_counted_separately_and_never_verifiable() {
        let m = metrics_of(vec![assume(), assume()]);
        assert_eq!(m.assumes, 2);
        assert_eq!(m.intents, 0);
        assert_eq!(m.verifiable, 0);
    }

    #[test]
    fn verify_false_intents_are_not_verifiable() {
        let m = metrics_of(vec![
            intent(VerifyLevel::Bool(false), Status::Unknown),
            intent(VerifyLevel::Method(VerifyMethod::Test), Status::Unknown),
        ]);
        assert_eq!(m.intents, 2, "both are intents");
        assert_eq!(m.verifiable, 1, "only the non-false one is verifiable");
    }

    #[test]
    fn verified_clean_counts_only_terminal_clean_verifiable_intents() {
        let m = metrics_of(vec![
            intent(VerifyLevel::Method(VerifyMethod::Neural), Status::Neural), // clean
            intent(VerifyLevel::Method(VerifyMethod::Test), Status::Tested),   // clean
            intent(VerifyLevel::Method(VerifyMethod::Full), Status::Verified), // clean
            intent(VerifyLevel::Method(VerifyMethod::Neural), Status::Stale),  // not clean
            intent(VerifyLevel::Method(VerifyMethod::Test), Status::Unknown),  // not clean
            intent(VerifyLevel::Bool(false), Status::Verified), // not verifiable (excluded)
        ]);
        assert_eq!(m.intents, 6);
        assert_eq!(m.verifiable, 5);
        assert_eq!(m.verified_clean, 3);
        assert_eq!(m.unverified, 2);
        assert!((m.verification_rate - 0.6).abs() < 1e-9);
    }

    #[test]
    fn unverified_plus_verified_clean_equals_verifiable() {
        let m = metrics_of(vec![
            intent(VerifyLevel::Method(VerifyMethod::Neural), Status::Neural),
            intent(
                VerifyLevel::Method(VerifyMethod::Test),
                Status::Counterexample,
            ),
            intent(VerifyLevel::Bool(true), Status::Unknown),
        ]);
        assert_eq!(m.verified_clean + m.unverified, m.verifiable);
    }

    #[test]
    fn metrics_json_round_trips() {
        let m = metrics_of(vec![intent(
            VerifyLevel::Method(VerifyMethod::Neural),
            Status::Neural,
        )]);
        let json = serde_json::to_string(&m).unwrap();
        let back: Metrics = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn metrics_schema_generates() {
        // The JsonSchema derive must produce a schema (gates the
        // `aristo metrics --json` contract test in the cli).
        let schema = schemars::schema_for!(Metrics);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(
            json.contains("verification_rate"),
            "schema names the fields"
        );
        assert!(json.contains("verified_clean"));
    }
}
