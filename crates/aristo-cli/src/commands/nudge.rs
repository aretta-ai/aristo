//! `aristo nudge` — the nudge/progress engine's union function + (S0d) the
//! introspection readout. The hook-driven emitter (`--event`) and throttle
//! land in the next slice; this one wires the engine end-to-end and lets a
//! human (or the #12 status skill) see what the engine *would* surface.
//!
//! The union function [`build_inputs`] is the COMPUTE join the scorer reads:
//! the index-derived [`Metrics`] plus the runtime facts only the cli can see
//! (reviewed map, proof-reviewed map, edit-window baseline, sign-in). It is
//! read-only and never fails the caller on missing pieces — a nudge surface
//! must degrade quietly, never break a workflow.

use aristo_core::config::ConfigFile;
use aristo_core::index::IndexEntry;
use aristo_core::metrics::Metrics;
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::nudge::state::{NudgeState, STATE_FILENAME};
use crate::nudge::{score, Audience, EngineInputs};
use crate::{CliError, CliResult, Workspace};

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let config = ws.load_config();
    let state = NudgeState::load(&ws.aristo_dir().join(STATE_FILENAME));
    let inputs = build_inputs(&ws, &config, &state)?;
    let aggressiveness = config.nudges.aggressiveness;
    let decision = score(&inputs, aggressiveness);
    print_readout(&inputs, aggressiveness, &decision);
    Ok(())
}

#[aristo::intent(
    "The union function is read-only and tolerant: it never mutates the \
     workspace and never fails the caller on missing runtime state. Absent \
     reviewed/proof-reviewed maps make everything read as unreviewed, an \
     absent baseline disables the gain/slump signals, and an unreadable \
     proofs dir contributes zero — degrade quietly. A nudge surface that \
     errored or wrote files would turn an advisory into a workflow blocker, \
     violating the engine's nudge-only posture (D3).",
    verify = "neural",
    id = "nudge_union_is_read_only_and_tolerant"
)]
/// Join the index-derived metrics with the cli-resident runtime signals into
/// the [`EngineInputs`] the scorer consumes. Read-only; tolerant of missing
/// state (the engine simply sees more as unreviewed / no baseline).
pub(crate) fn build_inputs(
    ws: &Workspace,
    config: &ConfigFile,
    state: &NudgeState,
) -> CliResult<EngineInputs> {
    let index = read_index(&ws.index_path())?;

    // Coverage denominator for the tier formula (read-only walk, as in badge).
    let fn_counts =
        count_fns_per_module_with(&ws.root, &WalkOptions::none()).map_err(|e| CliError::Other {
            message: format!("failed to walk source for metrics coverage: {e}"),
            exit_code: 1,
        })?;
    let metrics = Metrics::from_index(&index, &fn_counts, config.verify.default_method);

    // Unreviewed authored intents: every index intent the reviewed map doesn't
    // currently vouch for (absent, unmarked, or hash-drifted).
    let intent_keys: Vec<(String, String, String)> = index
        .entries
        .iter()
        .filter_map(|(id, entry)| match entry {
            IndexEntry::Intent(e) => Some((
                id.as_str().to_string(),
                e.text_hash.as_str().to_string(),
                e.body_hash.as_str().to_string(),
            )),
            IndexEntry::Assume(_) => None,
        })
        .collect();
    let unreviewed_intents = state.unreviewed_count(
        intent_keys
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str())),
    );

    let proofs_awaiting_review = count_proofs_awaiting(ws, state);

    let (prior_score, tier_increased) = match &state.baseline {
        Some(b) => (
            Some(b.score),
            metrics.tier.label() != b.tier && metrics.visible_score > b.score,
        ),
        None => (None, false),
    };

    // Local-only sign-in check (env var / config files; never network).
    let signed_in = aristo_core::auth::resolve_full().is_ok();

    Ok(EngineInputs {
        metrics,
        edits_since_annotation: state.edits_since_annotation,
        unreviewed_intents,
        proofs_awaiting_review,
        // TODO(S0d follow-up): read pending canon matches + suggestions from
        // the local `.aristo/canon-matches.toml` + suggestions queue. Until
        // then the (signed-in-gated) canon signal stays silent.
        canon_pending: 0,
        prior_score,
        tier_increased,
        signed_in,
    })
}

/// Count `.aristo/proofs/<id>.proof` files whose verdict the proof-reviewed
/// map doesn't yet vouch for. Absent dir → 0.
fn count_proofs_awaiting(ws: &Workspace, state: &NudgeState) -> usize {
    let dir = ws.aristo_dir().join("proofs");
    let Ok(read) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut n = 0usize;
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("proof") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = stem.replace("__", ":");
        if !state.proof_reviewed.get(&id).copied().unwrap_or(false) {
            n += 1;
        }
    }
    n
}

fn print_readout(
    inputs: &EngineInputs,
    aggressiveness: aristo_core::config::Aggressiveness,
    decision: &crate::nudge::Decision,
) {
    println!("Aristo nudge engine — would-surface readout");
    println!("  aggressiveness: {aggressiveness:?}");
    println!(
        "  inputs: {} unreviewed · {} unverified/{} verifiable · {} proofs awaiting · score {:.2} ({})",
        inputs.unreviewed_intents,
        inputs.metrics.unverified,
        inputs.metrics.verifiable,
        inputs.proofs_awaiting_review,
        inputs.metrics.visible_score,
        inputs.metrics.tier.label(),
    );
    if decision.is_silent() {
        println!("  → nothing would fire.");
        return;
    }
    if let Some(rec) = decision.recommended() {
        println!("  → recommended: {rec}");
    }
    for fired in &decision.human {
        println!("  · [human] {} (pressure {:.2})", fired.id, fired.pressure);
    }
    for fired in &decision.agent {
        let _ = Audience::Agent; // audiences are surfaced on different channels
        println!("  · [agent] {} (pressure {:.2})", fired.id, fired.pressure);
    }
}
