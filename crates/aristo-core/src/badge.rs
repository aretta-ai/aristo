//! Badge tier scoring — D7 visible-score formula + D8 tier cutoffs.
//!
//! The headline output `aristo badge` displays for a project is its
//! [`Tier`] — a five-step ladder grading the project's verification
//! posture (Aspirant → Apprentice → Adept → Ascendent → Areté, the
//! last hidden). Tier is a discrete state computed from two continuous
//! signals plus one discrete gate, per the locked decision doc
//! `docs/decisions/badge-tier-scheme.md`:
//!
//! 1. **Verification ratio** (D7): the share of verifiable intents
//!    currently in clean-verified status, weighted by verification
//!    depth (full = 1.0, test = 0.8, neural = 0.6, `verify=false`
//!    excluded entirely).
//! 2. **Coverage score** (D7): the share of modules that have at
//!    least their per-module target of intents, where target scales
//!    with module size as `⌈√fn_count(m)⌉`. Modules with zero
//!    functions are skipped — they have no behavioural surface.
//! 3. **Articulation floor** (D7): a small cold-start credit
//!    (max 0.15) for the act of writing intents, even unverified.
//!    Keeps brand-new projects in Apprentice range while they
//!    bootstrap their verification flow.
//! 4. **Areté gate** (D4): one `verify="full"` intent in clean
//!    Verified status AND server-bound. Unlocks the hidden Areté
//!    tier; without it, the visible scale saturates at Ascendent.
//!
//! `visible_score = max(articulation_floor, verification_ratio × coverage_score)`
//!
//! Lives in `aristo-core` (not `aristo-cli`) so future per-language
//! SDKs share the same scoring logic — the CLI badge command is the
//! first consumer but won't be the only one.
//!
//! Calibration posture is **`harshest-yet-realistic`** (D9): the
//! locked cutoffs (0.10 / 0.35 / 0.65) and target scaling (sqrt) err
//! on the side of underrating projects. Loosening is a future-tunable
//! post-launch knob; tightening is locked off forever (mirrors the
//! airline-miles-devaluation UX failure pattern).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::index::{IdNamespace, IndexEntry, IndexFile, Status, VerifyLevel, VerifyMethod};

/// Cold-start articulation floor — maximum number of intents that
/// count toward the baseline credit.
const ART_CAP: usize = 3;

/// Articulation credit per intent, up to [`ART_CAP`]. `ART_CAP *
/// ART_PER_INTENT = 0.15`, which keeps a brand-new project in
/// Apprentice range (≥ 0.10) without giving them Adept (≥ 0.35) on
/// articulation alone.
const ART_PER_INTENT: f64 = 0.05;

/// Tier cutoffs locked in D8. Lower-inclusive, upper-exclusive except
/// the top tier which is upper-inclusive.
const APPRENTICE_CUTOFF: f64 = 0.10;
const ADEPT_CUTOFF: f64 = 0.35;
const ASCENDENT_CUTOFF: f64 = 0.65;

/// The five-step ladder grading verification posture. Areté is a
/// discrete state change (D4 gate met), not a continuous gradient.
/// Visible-only projects (free-tier or paid-but-not-yet-server-bound)
/// saturate at [`Tier::Ascendent`] regardless of how high the
/// numeric score climbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// Seeking the path; has annotations, minimal verification.
    /// Score in `[0, 0.10)`.
    Aspirant,
    /// Learning the practice; lint + critique passes.
    /// Score in `[0.10, 0.35)`.
    Apprentice,
    /// Demonstrating skill; meaningful verification coverage.
    /// Score in `[0.35, 0.65)`.
    Adept,
    /// Rising toward areté; near-full verification.
    /// Score in `[0.65, 1.0]`; visible ceiling for free-tier.
    Ascendent,
    /// Excellence achieved; paid-tier formal proofs, server-bound.
    /// Hidden from free-tier badges by design (D3).
    Arete,
}

impl Tier {
    /// Human-readable label rendered in the SVG value half and
    /// progress lines. Areté carries a leading ✦ glyph (D11) so it
    /// reads as the discrete state change it is.
    pub fn label(self) -> &'static str {
        match self {
            Tier::Aspirant => "Aspirant",
            Tier::Apprentice => "Apprentice",
            Tier::Adept => "Adept",
            Tier::Ascendent => "Ascendent",
            Tier::Arete => "✦ Areté",
        }
    }

    /// Hex color for the value-half of the badge SVG. Palette locked
    /// in D11 — bridge primer through International Orange through
    /// sunset bridge through warm gold.
    pub fn color_hex(self) -> &'static str {
        match self {
            Tier::Aspirant => "#8a8378",
            Tier::Apprentice => "#c9a87c",
            Tier::Adept => "#C0362C",
            Tier::Ascendent => "#8c2913",
            Tier::Arete => "#d4a017",
        }
    }
}

/// Full breakdown of the badge computation. Returned so callers can
/// show the diagnostic surface (progress lines reporting score
/// components) AND consume the final [`Tier`] for the SVG.
#[derive(Debug, Clone)]
pub struct TierComputation {
    /// Number of intents where `verify != false` — the denominator
    /// of `verification_ratio` and the numerator of
    /// `articulation_floor`.
    pub verifiable: usize,
    /// Depth-weighted share of verifiable intents that are in
    /// clean-verified status. `0.0` when `verifiable == 0`.
    pub verification_ratio: f64,
    /// Module-aware coverage. `0.0` when no module has any
    /// functions in the production-counted surface.
    pub coverage_score: f64,
    /// Baseline credit for having articulated intents at all.
    pub articulation_floor: f64,
    /// `max(articulation_floor, verification_ratio × coverage_score)`.
    /// Clamped to `[0.0, 1.0]`.
    pub visible_score: f64,
    /// True iff D4 met: ≥1 `verify="full"` intent, `Status::Verified`,
    /// in the `aristos:` namespace.
    pub arete_gate_met: bool,
    /// The tier the project lands in. Visible scale saturates at
    /// `Ascendent` when the gate isn't met; `Arete` is only ever
    /// returned when the gate IS met.
    pub tier: Tier,
}

/// Map a numeric visible score to the visible-scale tier, ignoring
/// the Areté gate. The gate is applied by [`compute_tier`] after the
/// scale lookup.
pub fn score_to_visible_tier(score: f64) -> Tier {
    if score < APPRENTICE_CUTOFF {
        Tier::Aspirant
    } else if score < ADEPT_CUTOFF {
        Tier::Apprentice
    } else if score < ASCENDENT_CUTOFF {
        Tier::Adept
    } else {
        Tier::Ascendent
    }
}

/// Compute the full tier breakdown for an index against a set of
/// per-module function counts. `default_method` is the fallback when
/// an entry has `verify = true` (per the `[verify] default_method`
/// config field); use `None` to score `verify = true` as zero (the
/// conservative posture matching projects with no
/// `default_method` set).
///
/// Empty index → `Tier::Aspirant` at score 0.0; the formula is
/// total-order-preserving and any zero-intent project lands in the
/// floor tier.
pub fn compute_tier(
    index: &IndexFile,
    fn_counts: &BTreeMap<PathBuf, u32>,
    default_method: Option<VerifyMethod>,
) -> TierComputation {
    // ─── verification_ratio ──────────────────────────────────────────
    let mut verifiable = 0usize;
    let mut contribution_sum = 0.0f64;
    let mut arete_gate_met = false;
    // Track verifiable-intent count per source file for the
    // coverage_score denominator.
    let mut intents_by_file: BTreeMap<PathBuf, u32> = BTreeMap::new();

    for (id, entry) in &index.entries {
        let IndexEntry::Intent(e) = entry else {
            // Assumes excluded entirely (A5).
            continue;
        };
        if matches!(e.verify, VerifyLevel::Bool(false)) {
            // verify=false intents are documentation-only — never
            // verified, never counted.
            continue;
        }
        verifiable += 1;
        let resolved = resolve_verify(&e.verify, default_method);
        let weight = depth_weight(resolved, e.status);
        contribution_sum += weight;
        *intents_by_file.entry(PathBuf::from(&e.file)).or_default() += 1;

        // D4 gate: full + Verified + aristos: namespace.
        if matches!(resolved, Some(VerifyMethod::Full))
            && e.status == Status::Verified
            && matches!(id.namespace(), IdNamespace::Aristos)
        {
            arete_gate_met = true;
        }
    }

    let verification_ratio = if verifiable == 0 {
        0.0
    } else {
        contribution_sum / verifiable as f64
    };

    // ─── coverage_score ──────────────────────────────────────────────
    let coverage_score = compute_coverage_score(fn_counts, &intents_by_file);

    // ─── articulation_floor ──────────────────────────────────────────
    let articulation_floor = (verifiable.min(ART_CAP)) as f64 * ART_PER_INTENT;

    // ─── visible_score ───────────────────────────────────────────────
    let raw = (verification_ratio * coverage_score).max(articulation_floor);
    let visible_score = raw.clamp(0.0, 1.0);

    let visible_tier = score_to_visible_tier(visible_score);
    let tier = if arete_gate_met {
        Tier::Arete
    } else {
        visible_tier
    };

    TierComputation {
        verifiable,
        verification_ratio,
        coverage_score,
        articulation_floor,
        visible_score,
        arete_gate_met,
        tier,
    }
}

fn compute_coverage_score(
    fn_counts: &BTreeMap<PathBuf, u32>,
    intents_by_file: &BTreeMap<PathBuf, u32>,
) -> f64 {
    let mut local_credits = Vec::new();
    for (module, &fn_count) in fn_counts {
        if fn_count == 0 {
            // Pure-type modules — no behavioural surface, skip
            // entirely from the denominator (D7 explicit).
            continue;
        }
        let target = ((fn_count as f64).sqrt().ceil() as u32).max(1);
        let intents_here = intents_by_file.get(module.as_path()).copied().unwrap_or(0);
        let local = (intents_here as f64 / target as f64).min(1.0);
        local_credits.push(local);
    }

    if local_credits.is_empty() {
        return 0.0;
    }
    local_credits.iter().copied().sum::<f64>() / local_credits.len() as f64
}

fn resolve_verify(
    level: &VerifyLevel,
    default_method: Option<VerifyMethod>,
) -> Option<VerifyMethod> {
    match level {
        VerifyLevel::Bool(false) => None,
        VerifyLevel::Bool(true) => default_method,
        VerifyLevel::Method(m) => Some(*m),
    }
}

/// Depth weight per D7:
/// - clean-verified status (Verified / Tested / Neural) AND named
///   method → method weight (full = 1.0, test = 0.8, neural = 0.6).
/// - `verify=true` with no default_method resolved → 0 contribution.
/// - any other status (Unknown / Stale / Counterexample / etc.) → 0.
fn depth_weight(resolved: Option<VerifyMethod>, status: Status) -> f64 {
    if !matches!(status, Status::Verified | Status::Tested | Status::Neural) {
        return 0.0;
    }
    match resolved {
        Some(VerifyMethod::Full) => 1.0,
        Some(VerifyMethod::Test) => 0.8,
        Some(VerifyMethod::Neural) => 0.6,
        None => 0.0,
    }
}

/// Helper for the badge command: given a workspace `root`, run the
/// fn-count walker and return the result. Lives here (not in the
/// CLI) so the same walk-then-score sequence is one call for
/// future per-language SDK callers.
pub fn count_fns_for_root(
    root: &Path,
    opts: &crate::walk::WalkOptions,
) -> Result<BTreeMap<PathBuf, u32>, crate::walk::FsWalkError> {
    crate::walk::count_fns_per_module_with(root, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{
        AnnotationId, ArtaId, AssumeEntry, BindingState, CommitHash, CoveredRegion, IntentEntry,
        Meta, Sha256, VerifiedOutcome,
    };

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(file: &str, verify: VerifyLevel, status: Status, server_bound: bool) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: file.into(),
            site: format!("fn x in {file}"),
            covered_region: CoveredRegion::Function,
            binding: if server_bound {
                BindingState::Certified {
                    linked: ArtaId::parse("arta_op4q3z9NbV").unwrap(),
                    verified_outcome: VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86)))
                        .unwrap(),
                    last_verified_at_commit: CommitHash::parse(&"a".repeat(40)).unwrap(),
                }
            } else {
                BindingState::Local
            },
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
            site: "fn y".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn make_index(entries: Vec<(&str, IndexEntry)>) -> IndexFile {
        let mut map = std::collections::BTreeMap::new();
        for (id, e) in entries {
            map.insert(AnnotationId::parse(id).unwrap(), e);
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

    fn fn_counts(pairs: &[(&str, u32)]) -> BTreeMap<PathBuf, u32> {
        pairs.iter().map(|(p, n)| (PathBuf::from(p), *n)).collect()
    }

    // ─── Tier surface ────────────────────────────────────────────────

    #[test]
    fn tier_labels_match_d1_scheme() {
        assert_eq!(Tier::Aspirant.label(), "Aspirant");
        assert_eq!(Tier::Apprentice.label(), "Apprentice");
        assert_eq!(Tier::Adept.label(), "Adept");
        assert_eq!(Tier::Ascendent.label(), "Ascendent");
        assert_eq!(Tier::Arete.label(), "✦ Areté");
    }

    #[test]
    fn tier_colors_match_d11_palette() {
        assert_eq!(Tier::Adept.color_hex(), "#C0362C"); // International Orange
        assert_eq!(Tier::Arete.color_hex(), "#d4a017");
        // Just spot-check the rest exist + are distinct.
        let all = [
            Tier::Aspirant.color_hex(),
            Tier::Apprentice.color_hex(),
            Tier::Adept.color_hex(),
            Tier::Ascendent.color_hex(),
            Tier::Arete.color_hex(),
        ];
        for c in all {
            assert!(c.starts_with('#'));
            assert_eq!(c.len(), 7);
        }
        let unique: std::collections::HashSet<&str> = all.into_iter().collect();
        assert_eq!(unique.len(), 5, "tier colors must be distinct");
    }

    #[test]
    fn score_to_visible_tier_respects_d8_cutoffs() {
        assert_eq!(score_to_visible_tier(0.00), Tier::Aspirant);
        assert_eq!(score_to_visible_tier(0.099), Tier::Aspirant);
        assert_eq!(score_to_visible_tier(0.10), Tier::Apprentice);
        assert_eq!(score_to_visible_tier(0.349), Tier::Apprentice);
        assert_eq!(score_to_visible_tier(0.35), Tier::Adept);
        assert_eq!(score_to_visible_tier(0.649), Tier::Adept);
        assert_eq!(score_to_visible_tier(0.65), Tier::Ascendent);
        assert_eq!(score_to_visible_tier(1.0), Tier::Ascendent);
    }

    // ─── compute_tier — empty + cold-start ───────────────────────────

    #[test]
    fn empty_index_is_aspirant_with_zero_score() {
        let r = compute_tier(&make_index(vec![]), &BTreeMap::new(), None);
        assert_eq!(r.verifiable, 0);
        assert_eq!(r.verification_ratio, 0.0);
        assert_eq!(r.coverage_score, 0.0);
        assert_eq!(r.articulation_floor, 0.0);
        assert_eq!(r.visible_score, 0.0);
        assert!(!r.arete_gate_met);
        assert_eq!(r.tier, Tier::Aspirant);
    }

    #[test]
    fn assumes_excluded_from_verifiable_count() {
        let index = make_index(vec![
            ("a", assume()),
            ("b", assume()),
            (
                "c",
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ),
        ]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        // Two assumes invisible to scoring; one intent contributes.
        assert_eq!(r.verifiable, 1);
    }

    #[test]
    fn verify_false_intents_excluded_entirely() {
        let index = make_index(vec![
            (
                "a",
                intent(
                    "src/lib.rs",
                    VerifyLevel::Bool(false),
                    Status::Unknown,
                    false,
                ),
            ),
            (
                "b",
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ),
        ]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        // verify=false excluded entirely — denominator is 1, not 2.
        assert_eq!(r.verifiable, 1);
        assert!((r.verification_ratio - 0.6).abs() < 1e-9);
    }

    // ─── articulation_floor ──────────────────────────────────────────

    #[test]
    fn articulation_floor_one_intent_gives_five_pct() {
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Bool(true),
                Status::Unknown,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!((r.articulation_floor - 0.05).abs() < 1e-9);
        // 1 unverified intent → score = 0.05 (from floor) → Aspirant.
        assert_eq!(r.tier, Tier::Aspirant);
    }

    #[test]
    fn articulation_floor_saturates_at_three_intents() {
        let mut entries = Vec::new();
        for i in 0..10 {
            let id = format!("intent_{i}");
            // verify=true with no default_method → unresolved, 0 contribution
            entries.push((
                Box::leak(id.into_boxed_str()) as &str,
                intent(
                    "src/lib.rs",
                    VerifyLevel::Bool(true),
                    Status::Unknown,
                    false,
                ),
            ));
        }
        let index = make_index(entries);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        // 10 unverified intents → floor caps at 0.15 → Apprentice.
        assert!((r.articulation_floor - 0.15).abs() < 1e-9);
        assert_eq!(r.tier, Tier::Apprentice);
    }

    // ─── verification_ratio ──────────────────────────────────────────

    #[test]
    fn full_verified_intent_contributes_one_point_oh() {
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!((r.verification_ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_verified_intent_contributes_zero_eight() {
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Test),
                Status::Tested,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!((r.verification_ratio - 0.8).abs() < 1e-9);
    }

    #[test]
    fn neural_verified_intent_contributes_zero_six() {
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!((r.verification_ratio - 0.6).abs() < 1e-9);
    }

    #[test]
    fn non_clean_verified_status_contributes_zero() {
        let dirty = [
            Status::Unknown,
            Status::Stale,
            Status::Orphan,
            Status::Forged,
            Status::Counterexample,
            Status::PendingDeepen,
            Status::Inconclusive,
        ];
        for s in dirty {
            let index = make_index(vec![(
                "a",
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Full),
                    s,
                    false,
                ),
            )]);
            let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
            assert_eq!(
                r.verification_ratio, 0.0,
                "status {s:?} should contribute 0 to verification_ratio"
            );
        }
    }

    #[test]
    fn verify_true_uses_default_method_for_weight() {
        let index = make_index(vec![(
            "a",
            intent("src/lib.rs", VerifyLevel::Bool(true), Status::Tested, false),
        )]);
        // With default_method = Some(Test) → weight 0.8.
        let r = compute_tier(
            &index,
            &fn_counts(&[("src/lib.rs", 1)]),
            Some(VerifyMethod::Test),
        );
        assert!((r.verification_ratio - 0.8).abs() < 1e-9);
        // With default_method = None → unresolved, weight 0.
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert_eq!(r.verification_ratio, 0.0);
    }

    // ─── coverage_score ──────────────────────────────────────────────

    #[test]
    fn coverage_score_zero_when_no_fn_modules() {
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                false,
            ),
        )]);
        // Zero-fn module is skipped from denominator → coverage_score 0.
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 0)]), None);
        assert_eq!(r.coverage_score, 0.0);
    }

    #[test]
    fn coverage_score_saturates_at_local_target() {
        // 4 fns → target ⌈√4⌉ = 2; 5 intents → local_credit = min(5/2, 1) = 1.0.
        let mut entries = Vec::new();
        for i in 0..5 {
            let id = format!("intent_{i}");
            entries.push((
                Box::leak(id.into_boxed_str()) as &str,
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ));
        }
        let r = compute_tier(&make_index(entries), &fn_counts(&[("src/lib.rs", 4)]), None);
        assert!((r.coverage_score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_score_averages_per_module() {
        // Two modules with 4 fns each; one has 2 intents (saturated), other has 0.
        // local_credit(a) = min(2/2, 1) = 1.0; local_credit(b) = 0/2 = 0.0;
        // coverage_score = (1.0 + 0.0) / 2 = 0.5.
        let index = make_index(vec![
            (
                "a",
                intent(
                    "src/a.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ),
            (
                "b",
                intent(
                    "src/a.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ),
        ]);
        let r = compute_tier(
            &index,
            &fn_counts(&[("src/a.rs", 4), ("src/b.rs", 4)]),
            None,
        );
        assert!((r.coverage_score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn coverage_target_uses_ceiling_of_sqrt() {
        // 10 fns → ⌈√10⌉ = 4. With 4 intents → saturated.
        let mut entries = Vec::new();
        for i in 0..4 {
            let id = format!("intent_{i}");
            entries.push((
                Box::leak(id.into_boxed_str()) as &str,
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ));
        }
        let r = compute_tier(
            &make_index(entries),
            &fn_counts(&[("src/lib.rs", 10)]),
            None,
        );
        assert!((r.coverage_score - 1.0).abs() < 1e-9);
        // With 3 intents → 3/4 = 0.75.
        let mut entries = Vec::new();
        for i in 0..3 {
            let id = format!("intent_{i}");
            entries.push((
                Box::leak(id.into_boxed_str()) as &str,
                intent(
                    "src/lib.rs",
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ));
        }
        let r = compute_tier(
            &make_index(entries),
            &fn_counts(&[("src/lib.rs", 10)]),
            None,
        );
        assert!((r.coverage_score - 0.75).abs() < 1e-9);
    }

    // ─── visible_score combination + Areté gate ──────────────────────

    #[test]
    fn visible_score_takes_max_of_floor_and_product() {
        // 1 intent verified-neural in a 1-fn project:
        //   verification_ratio = 0.6 → product = 0.6 × 1.0 = 0.6
        //   articulation_floor = 1 × 0.05 = 0.05
        // max → 0.6. Tier should be Adept.
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!((r.visible_score - 0.6).abs() < 1e-9);
        assert_eq!(r.tier, Tier::Adept);
    }

    #[test]
    fn visible_score_clamps_to_one() {
        // Manufacture an over-1 raw product: 1.0 ratio × 1.0 coverage = 1.0.
        // (The formula can't actually exceed 1.0 organically, but the clamp
        // is a defensive belt-and-suspenders.)
        let index = make_index(vec![(
            "a",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!(r.visible_score <= 1.0 + 1e-9);
        assert!(r.visible_score >= 1.0 - 1e-9);
        assert_eq!(r.tier, Tier::Ascendent);
    }

    #[test]
    fn arete_gate_requires_full_verified_and_aristos_namespace() {
        // Server-bound + verify=full + Verified → Areté.
        let index = make_index(vec![(
            "aristos:foo",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                true,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!(r.arete_gate_met);
        assert_eq!(r.tier, Tier::Arete);
    }

    #[test]
    fn arete_gate_local_namespace_does_not_unlock() {
        // verify=full + Verified but local namespace (no aristos:) → no Areté.
        let index = make_index(vec![(
            "foo",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Verified,
                false,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!(!r.arete_gate_met);
        // High visible score → Ascendent ceiling.
        assert_eq!(r.tier, Tier::Ascendent);
    }

    #[test]
    fn arete_gate_non_full_method_does_not_unlock_even_with_namespace() {
        // aristos: namespace but verify=neural — D4 requires the FULL
        // method, not just being server-bound.
        let index = make_index(vec![(
            "aristos:foo",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                true,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!(!r.arete_gate_met);
        assert_eq!(r.tier, Tier::Adept); // 0.6 visible_score → Adept
    }

    #[test]
    fn arete_gate_non_verified_status_does_not_unlock() {
        // Server-bound aristos: + full + STALE → no Areté (status not verified).
        let index = make_index(vec![(
            "aristos:foo",
            intent(
                "src/lib.rs",
                VerifyLevel::Method(VerifyMethod::Full),
                Status::Stale,
                true,
            ),
        )]);
        let r = compute_tier(&index, &fn_counts(&[("src/lib.rs", 1)]), None);
        assert!(!r.arete_gate_met);
        // Stale → 0 contribution → only floor (0.05) → Aspirant.
        assert_eq!(r.tier, Tier::Aspirant);
    }
}
