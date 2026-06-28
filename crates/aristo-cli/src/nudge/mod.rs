//! The nudge/progress engine's DECIDE leg (Phase 18 #9, S0c).
//!
//! A plug-and-play scorer over a registry of `Signal`s. Each signal turns
//! some engine input into a raw "pressure" `m`; the scorer normalizes it by
//! the signal's base `b` and scales by the configured aggressiveness factor
//! `f`, firing iff `pressure * f >= 1`. Adding a nudge is one `SIGNALS`
//! entry — the scorer, ordering, and (later) throttle/render are generic.
//!
//! Two design invariants this module enforces:
//!
//! - **`aggressiveness = off` is a hard silence.** `factor()` is `0.0`, so
//!   `pressure * 0.0 >= 1` is false for every signal at every pressure —
//!   nothing can fire (cf. the `aggressiveness_off_is_hard_silence` intent
//!   in `aristo-core::config`).
//! - **Ordering is fixed priority, not pressure.** Pressures of counts and
//!   fractions are incommensurable, so the surfaced order is the static
//!   `SIGNALS` order (congrats banner, then review > canon > verify >
//!   proof-review > slump); the Agent-audience signal is a separate surface.

use aristo_core::config::Aggressiveness;
use aristo_core::metrics::Metrics;

pub mod intents;
pub mod state;
pub mod throttle;

/// Who a fired signal is addressed to. The two are surfaced through
/// different channels (D4/D10): the agent gets an inline `<system-reminder>`;
/// the human gets the consolidated review prompt the agent pops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Nudge the coding agent directly (e.g. "you haven't annotated lately").
    Agent,
    /// Surface to the human via the agent-run review prompt.
    Human,
}

/// Everything the signal metrics read. The cli's union function (S0c+)
/// populates the runtime fields it can't see from the index alone (reviewed
/// map, queue/canon counts, the edit-window baseline); the index-derived
/// part is [`Metrics`].
#[derive(Debug, Clone)]
pub struct EngineInputs {
    /// Index-derived metrics (counts, backlog, tier, score).
    pub metrics: Metrics,
    /// Source edits since the last annotation was added (PostToolUse counter).
    pub edits_since_annotation: usize,
    /// Authored intents not yet marked reviewed (new + backlog).
    pub unreviewed_intents: usize,
    /// Proofs carrying a verdict not yet in the proof-reviewed map.
    pub proofs_awaiting_review: usize,
    /// Pending canon matches + suggestions (only meaningful when signed in).
    pub canon_pending: usize,
    /// The visible score at the edit-window baseline, if one was captured.
    pub prior_score: Option<f64>,
    /// Whether the tier rose since the edit-window baseline (instant win).
    pub tier_increased: bool,
    /// Whether a repo-scoped token is present (gates the paid canon signal).
    pub signed_in: bool,
}

/// A registered nudge. `metric` extracts the raw pressure numerator `m` from
/// the inputs; `base` is the denominator `b`; the signal fires when
/// `(m / b) * factor >= 1`.
pub struct Signal {
    /// Stable identifier (also the throttle-map key).
    pub id: &'static str,
    /// Who the nudge is for.
    pub audience: Audience,
    /// The pressure denominator `b` (> 0).
    pub base: f64,
    /// Extract the raw pressure numerator `m` from the inputs.
    pub metric: fn(&EngineInputs) -> f64,
}

impl Signal {
    /// Normalized pressure `m / b` (independent of aggressiveness).
    pub fn pressure(&self, inputs: &EngineInputs) -> f64 {
        (self.metric)(inputs) / self.base
    }
}

/// A signal that fired, with the pressure that fired it and the raw
/// `metric`/`base` (so the throttle can apply a material-increase re-arm
/// against the same numbers).
#[derive(Debug, Clone, PartialEq)]
pub struct Fired {
    pub id: &'static str,
    pub audience: Audience,
    pub pressure: f64,
    /// The raw pressure numerator `m` (`pressure == metric / base`).
    pub metric: f64,
    /// The signal's pressure denominator `b`.
    pub base: f64,
}

/// The registry. Order IS priority for the human surface: the congrats
/// banner leads, then review > canon > verify > proof-review > slump. The
/// Agent-audience `authoring_debt` sits last (separate channel, order among
/// human signals irrelevant to it).
pub static SIGNALS: &[Signal] = &[
    Signal {
        id: "congrats",
        audience: Audience::Human,
        base: 0.05,
        metric: metric_congrats,
    },
    Signal {
        id: "review_backlog",
        audience: Audience::Human,
        base: 3.0,
        metric: metric_review_backlog,
    },
    Signal {
        id: "canon_pending",
        audience: Audience::Human,
        base: 3.0,
        metric: metric_canon_pending,
    },
    Signal {
        id: "verify_backlog",
        audience: Audience::Human,
        base: 0.25,
        metric: metric_verify_backlog,
    },
    Signal {
        id: "proof_review_backlog",
        audience: Audience::Human,
        base: 3.0,
        metric: metric_proof_review_backlog,
    },
    Signal {
        id: "score_slump",
        audience: Audience::Human,
        base: 0.05,
        metric: metric_score_slump,
    },
    Signal {
        id: "authoring_debt",
        audience: Audience::Agent,
        base: 3.0,
        metric: metric_authoring_debt,
    },
];

fn metric_authoring_debt(i: &EngineInputs) -> f64 {
    i.edits_since_annotation as f64
}

fn metric_review_backlog(i: &EngineInputs) -> f64 {
    i.unreviewed_intents as f64
}

fn metric_proof_review_backlog(i: &EngineInputs) -> f64 {
    i.proofs_awaiting_review as f64
}

fn metric_canon_pending(i: &EngineInputs) -> f64 {
    // Paid signal: silent unless signed in (no upsell spam).
    if i.signed_in {
        i.canon_pending as f64
    } else {
        0.0
    }
}

fn metric_verify_backlog(i: &EngineInputs) -> f64 {
    // Fraction of the verifiable surface still unverified.
    if i.metrics.verifiable == 0 {
        0.0
    } else {
        i.metrics.unverified as f64 / i.metrics.verifiable as f64
    }
}

fn metric_score_slump(i: &EngineInputs) -> f64 {
    // Relative drop from the edit-window baseline; 0 if no baseline or no drop.
    match i.prior_score {
        Some(prior) if prior > 0.0 => {
            let drop = prior - i.metrics.visible_score;
            if drop > 0.0 {
                drop / prior
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn metric_congrats(i: &EngineInputs) -> f64 {
    // A tier-up is an instant win — always congratulate (when nudges are on);
    // INFINITY pressure clears any non-zero factor's threshold but is still
    // silenced by factor 0.0 (off). Otherwise congratulate on a score gain.
    if i.tier_increased {
        return f64::INFINITY;
    }
    match i.prior_score {
        Some(prior) => (i.metrics.visible_score - prior).max(0.0),
        None => 0.0,
    }
}

/// The scorer's verdict: the fired signals in priority order, split by
/// audience, plus the recommended lead (the highest-priority fired human
/// signal — the engine kicks its low-friction/background action).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decision {
    /// Fired Human-audience signals, highest priority first.
    pub human: Vec<Fired>,
    /// Fired Agent-audience signals.
    pub agent: Vec<Fired>,
}

impl Decision {
    /// The recommended lead signal id — the top fired human signal, or `None`
    /// when nothing human-facing fired.
    pub fn recommended(&self) -> Option<&'static str> {
        self.human.first().map(|f| f.id)
    }

    /// True when nothing fired for either audience.
    pub fn is_silent(&self) -> bool {
        self.human.is_empty() && self.agent.is_empty()
    }
}

#[aristo::intent(
    "Two invariants the scorer must preserve. First, `aggressiveness = off` \
     is an absolute silence: its factor is 0.0 and the fire test is \
     `pressure * factor >= 1`, so no signal fires at any pressure — even an \
     infinite one. Second, the surfaced order is the static \
     SIGNALS priority order, NOT the pressures: a count-pressure and a \
     fraction-pressure are incommensurable, so sorting by pressure would let \
     a noisy low-priority signal jump the queue ahead of a review the user \
     actually needs to see first.",
    verify = "test",
    id = "nudge_scorer_off_silences_and_order_is_static_priority"
)]
pub fn score(inputs: &EngineInputs, aggressiveness: Aggressiveness) -> Decision {
    let factor = aggressiveness.factor();
    let mut decision = Decision::default();
    for signal in SIGNALS {
        let metric = (signal.metric)(inputs);
        let pressure = metric / signal.base;
        if pressure * factor >= 1.0 {
            let fired = Fired {
                id: signal.id,
                audience: signal.audience,
                pressure,
                metric,
                base: signal.base,
            };
            match signal.audience {
                Audience::Human => decision.human.push(fired),
                Audience::Agent => decision.agent.push(fired),
            }
        }
    }
    decision
}

#[aristo::intent(
    "Scoring the authoring-debt (agent) signal needs ONLY the edit counter — \
     never the index-derived Metrics. The PostToolUse hook that drives it \
     fires on every edit, so it must not walk the source tree per edit; this \
     scores the one signal straight from the counter, reusing the registry's \
     base and the identical `pressure * factor >= 1` fire rule so it can't \
     drift from `score`.",
    verify = "test",
    id = "score_authoring_debt_needs_no_index_walk"
)]
/// Score ONLY the `authoring_debt` signal, from the edit counter alone — no
/// `EngineInputs`/`Metrics` and so no source walk (PostToolUse fires on every
/// edit and must stay cheap). Returns the [`Fired`] (carrying metric + base
/// for the throttle) when it fires.
pub fn score_authoring_debt(
    edits_since_annotation: usize,
    aggressiveness: Aggressiveness,
) -> Option<Fired> {
    let signal = SIGNALS.iter().find(|s| s.id == "authoring_debt")?;
    let metric = edits_since_annotation as f64;
    let pressure = metric / signal.base;
    if pressure * aggressiveness.factor() >= 1.0 {
        Some(Fired {
            id: signal.id,
            audience: signal.audience,
            pressure,
            metric,
            base: signal.base,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::badge::Tier;
    use aristo_core::metrics::{Metrics, METRICS_SCHEMA_VERSION};

    fn metrics(verifiable: usize, unverified: usize, score: f64, tier: Tier) -> Metrics {
        Metrics {
            schema_version: METRICS_SCHEMA_VERSION,
            intents: verifiable,
            assumes: 0,
            verifiable,
            verified_clean: verifiable - unverified,
            unverified,
            verification_rate: if verifiable == 0 {
                0.0
            } else {
                (verifiable - unverified) as f64 / verifiable as f64
            },
            tier,
            visible_score: score,
            by_verify_level: Default::default(),
            status_distribution: Default::default(),
        }
    }

    fn inputs() -> EngineInputs {
        EngineInputs {
            metrics: metrics(0, 0, 0.0, Tier::Aspirant),
            edits_since_annotation: 0,
            unreviewed_intents: 0,
            proofs_awaiting_review: 0,
            canon_pending: 0,
            prior_score: None,
            tier_increased: false,
            signed_in: false,
        }
    }

    #[test]
    fn off_is_a_hard_silence_even_under_extreme_pressure() {
        let mut i = inputs();
        i.unreviewed_intents = 1000;
        i.edits_since_annotation = 1000;
        i.tier_increased = true;
        let d = score(&i, Aggressiveness::Off);
        assert!(d.is_silent(), "off must silence everything: {d:?}");
    }

    #[test]
    fn nothing_fires_at_zero_pressure() {
        let d = score(&inputs(), Aggressiveness::High);
        assert!(d.is_silent());
    }

    #[test]
    fn review_backlog_fires_at_base_under_medium() {
        let mut i = inputs();
        i.unreviewed_intents = 3; // pressure 3/3 = 1.0; *1.0 medium = 1.0 >= 1
        let d = score(&i, Aggressiveness::Medium);
        assert!(d.human.iter().any(|f| f.id == "review_backlog"));
    }

    #[test]
    fn low_aggressiveness_needs_more_pressure_than_medium() {
        let mut i = inputs();
        i.unreviewed_intents = 3; // 1.0 pressure
                                  // low factor 0.6 → 0.6 < 1 → does not fire
        assert!(score(&i, Aggressiveness::Low)
            .human
            .iter()
            .all(|f| f.id != "review_backlog"));
        // medium factor 1.0 → fires
        assert!(score(&i, Aggressiveness::Medium)
            .human
            .iter()
            .any(|f| f.id == "review_backlog"));
    }

    #[test]
    fn canon_pending_is_silent_until_signed_in() {
        let mut i = inputs();
        i.canon_pending = 10;
        assert!(score(&i, Aggressiveness::High)
            .human
            .iter()
            .all(|f| f.id != "canon_pending"));
        i.signed_in = true;
        assert!(score(&i, Aggressiveness::High)
            .human
            .iter()
            .any(|f| f.id == "canon_pending"));
    }

    #[test]
    fn tier_up_always_congratulates_when_on() {
        let mut i = inputs();
        i.tier_increased = true;
        let d = score(&i, Aggressiveness::Low);
        assert_eq!(
            d.recommended(),
            Some("congrats"),
            "tier-up leads as the banner"
        );
    }

    #[test]
    fn verify_backlog_uses_unverified_fraction() {
        let mut i = inputs();
        // 1 of 1 unverified → fraction 1.0; /0.25 = 4.0 pressure → fires easily.
        i.metrics = metrics(1, 1, 0.0, Tier::Aspirant);
        assert!(score(&i, Aggressiveness::Low)
            .human
            .iter()
            .any(|f| f.id == "verify_backlog"));
        // 0 unverified → no pressure.
        i.metrics = metrics(4, 0, 0.5, Tier::Adept);
        assert!(score(&i, Aggressiveness::High)
            .human
            .iter()
            .all(|f| f.id != "verify_backlog"));
    }

    #[test]
    fn score_slump_fires_on_a_relative_drop_from_baseline() {
        let mut i = inputs();
        i.prior_score = Some(0.50);
        i.metrics = metrics(0, 0, 0.40, Tier::Adept); // drop 0.10 / 0.50 = 0.20
                                                      // 0.20 / 0.05 = 4.0 pressure → fires.
        assert!(score(&i, Aggressiveness::Low)
            .human
            .iter()
            .any(|f| f.id == "score_slump"));
        // No drop → silent.
        i.metrics = metrics(0, 0, 0.60, Tier::Adept);
        assert!(score(&i, Aggressiveness::High)
            .human
            .iter()
            .all(|f| f.id != "score_slump"));
    }

    #[test]
    fn authoring_debt_is_agent_audience() {
        let mut i = inputs();
        i.edits_since_annotation = 3;
        let d = score(&i, Aggressiveness::Medium);
        assert!(d.agent.iter().any(|f| f.id == "authoring_debt"));
        assert!(d.human.iter().all(|f| f.id != "authoring_debt"));
    }

    #[test]
    fn score_authoring_debt_agrees_with_full_score() {
        // The counter-only fast path must match what the full scorer decides
        // for authoring_debt at the same edit count + aggressiveness.
        for edits in [0usize, 2, 3, 10] {
            for agg in [
                Aggressiveness::Off,
                Aggressiveness::Low,
                Aggressiveness::Medium,
                Aggressiveness::High,
            ] {
                let mut i = inputs();
                i.edits_since_annotation = edits;
                let full = score(&i, agg)
                    .agent
                    .iter()
                    .any(|f| f.id == "authoring_debt");
                let fast = score_authoring_debt(edits, agg).is_some();
                assert_eq!(full, fast, "edits={edits} agg={agg:?}");
            }
        }
        // Off is a hard silence even at a huge count.
        assert!(score_authoring_debt(1000, Aggressiveness::Off).is_none());
    }

    #[test]
    fn human_signals_keep_static_priority_order() {
        // Fire review, canon, and verify together; the surfaced order must be
        // review > canon > verify regardless of their pressures.
        let mut i = inputs();
        i.signed_in = true;
        i.unreviewed_intents = 100; // huge
        i.canon_pending = 3; // exactly base
        i.metrics = metrics(1, 1, 0.0, Tier::Aspirant); // verify fraction 1.0
        let d = score(&i, Aggressiveness::Medium);
        let order: Vec<&str> = d.human.iter().map(|f| f.id).collect();
        let review = order.iter().position(|x| *x == "review_backlog").unwrap();
        let canon = order.iter().position(|x| *x == "canon_pending").unwrap();
        let verify = order.iter().position(|x| *x == "verify_backlog").unwrap();
        assert!(
            review < canon && canon < verify,
            "priority order: {order:?}"
        );
        assert_eq!(d.recommended(), Some("review_backlog"));
    }
}
