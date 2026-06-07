//! `aristo nudge` — the nudge/progress engine's union function + surfaces
//! (Phase 18 #9, S0d). With no `--event` it prints what the engine would
//! surface (human introspection); with `--event <post-tool-use|stop|
//! session-start>` it runs as a Claude Code hook emitter and ALWAYS exits 0
//! (a nudge must never break the agent). The hook install + the live spike
//! that validates the Stop-hook contract land in S0d.3.
//!
//! The union function [`build_inputs`] is the COMPUTE join the scorer reads:
//! the index-derived [`Metrics`] plus the runtime facts only the cli can see
//! (reviewed map, proof-reviewed map, edit-window baseline, sign-in). It is
//! read-only and never fails the caller on missing pieces — a nudge surface
//! must degrade quietly, never break a workflow.

use std::io::Read;

use aristo_core::config::{Aggressiveness, ConfigFile};
use aristo_core::metrics::Metrics;
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::nudge::state::{Baseline, NudgeState, STATE_FILENAME};
use crate::nudge::{score, throttle, Audience, Decision, EngineInputs};
use crate::{CliError, CliResult, Workspace};

/// Which Claude Code hook event this invocation is serving (or `None` for the
/// human introspection readout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookEvent {
    PostToolUse,
    Stop,
    SessionStart,
}

fn parse_event(raw: &str) -> Option<HookEvent> {
    match raw {
        "post-tool-use" | "PostToolUse" => Some(HookEvent::PostToolUse),
        "stop" | "Stop" => Some(HookEvent::Stop),
        "session-start" | "SessionStart" => Some(HookEvent::SessionStart),
        _ => None,
    }
}

pub(crate) fn run(event: Option<String>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let config = ws.load_config();
    let aggressiveness = config.nudges.aggressiveness;

    // No --event → human introspection readout (the only mode that prints to
    // a human and may surface an error).
    let Some(raw) = event else {
        let state = NudgeState::load(&ws.aristo_dir().join(STATE_FILENAME));
        let inputs = build_inputs(&ws, &config, &state)?;
        let decision = score(&inputs, aggressiveness);
        print_readout(&inputs, aggressiveness, &decision);
        return Ok(());
    };

    // Hook mode: a nudge hook must NEVER break the agent. Swallow every error
    // and exit 0 — the worst case is "no nudge this turn".
    let _ = emit_for_event(&ws, &config, aggressiveness, &raw);
    Ok(())
}

fn emit_for_event(
    ws: &Workspace,
    config: &ConfigFile,
    aggressiveness: Aggressiveness,
    raw_event: &str,
) -> CliResult<()> {
    let Some(event) = parse_event(raw_event) else {
        return Ok(()); // unknown event → silent
    };
    let state_path = ws.aristo_dir().join(STATE_FILENAME);
    let mut state = NudgeState::load(&state_path);
    let now = now_epoch();

    match event {
        HookEvent::PostToolUse => {
            // Count edit-like tool calls toward the authoring-debt signal.
            if stdin_tool_is_edit() {
                state.edits_since_annotation = state.edits_since_annotation.saturating_add(1);
                let _ = state.save(&state_path);
            }
            if aggressiveness.is_off() {
                return Ok(());
            }
            let inputs = build_inputs(ws, config, &state)?;
            let decision = score(&inputs, aggressiveness);
            // Agent surface (authoring_debt). Throttle it too so it doesn't
            // nag on every edit once over threshold.
            if let Some(f) = decision.agent.iter().find(|f| f.id == "authoring_debt") {
                if throttle::may_surface(
                    state.throttle.get(f.id),
                    now,
                    aggressiveness,
                    f.metric,
                    f.base,
                ) {
                    emit_agent_reminder(inputs.edits_since_annotation);
                    state.throttle.insert(
                        f.id.to_string(),
                        throttle::record_after_surface(now, f.metric),
                    );
                    let _ = state.save(&state_path);
                }
            }
        }
        HookEvent::Stop => {
            if aggressiveness.is_off() {
                return Ok(());
            }
            let inputs = build_inputs(ws, config, &state)?;
            let decision = score(&inputs, aggressiveness);
            // Consolidated human nudge: surface if ANY fired human signal
            // clears its throttle. Update records for the ones that did.
            let cleared: Vec<crate::nudge::Fired> = decision
                .human
                .iter()
                .filter(|f| {
                    throttle::may_surface(
                        state.throttle.get(f.id),
                        now,
                        aggressiveness,
                        f.metric,
                        f.base,
                    )
                })
                .cloned()
                .collect();
            if !cleared.is_empty() {
                emit_stop_reminder(&decision, &inputs);
                for f in &cleared {
                    state.throttle.insert(
                        f.id.to_string(),
                        throttle::record_after_surface(now, f.metric),
                    );
                }
                let _ = state.save(&state_path);
            }
        }
        HookEvent::SessionStart => {
            // Capture the edit-window baseline + reset the edit counter, then
            // re-surface any standing backlog as additionalContext.
            let inputs = build_inputs(ws, config, &state)?;
            state.baseline = Some(Baseline {
                score: inputs.metrics.visible_score,
                tier: inputs.metrics.tier.label().to_string(),
            });
            // Snapshot the authored-intent id-set so #7 can split the review
            // backlog into new-this-session vs carried-over. Best-effort: if
            // the index can't be read, leave the window uncaptured (the split
            // is then suppressed, not guessed).
            state.window_intent_ids = read_index(&ws.index_path()).ok().map(|idx| {
                crate::nudge::intents::authored_intents(&idx)
                    .into_iter()
                    .map(|i| i.id)
                    .collect()
            });
            state.edits_since_annotation = 0;
            let _ = state.save(&state_path);
            if aggressiveness.is_off() {
                return Ok(());
            }
            let decision = score(&inputs, aggressiveness);
            if !decision.human.is_empty() {
                emit_session_start_context(&decision, &inputs);
            }
        }
    }
    Ok(())
}

fn now_epoch() -> u64 {
    time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64
}

/// Read the PostToolUse hook payload from stdin and decide whether the tool
/// was an edit-like (source-mutating) call. Tolerant: any parse failure or
/// absent stdin counts as "not an edit" (don't bump on uncertainty).
fn stdin_tool_is_edit() -> bool {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() || buf.trim().is_empty() {
        return false;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&buf) else {
        return false;
    };
    let tool = json.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
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
    // currently vouch for (absent, unmarked, or hash-drifted). The same
    // `authored_intents` enumeration `aristo review` reads, so the engine and
    // the review surface can never report a different backlog.
    let intents = crate::nudge::intents::authored_intents(&index);
    let unreviewed_intents = state.unreviewed_count(
        intents
            .iter()
            .map(|i| (i.id.as_str(), i.text_hash.as_str(), i.body_hash.as_str())),
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

    // Canon matching is paid (#10): only read the local cache + suggestions
    // queue when signed in, so the canon signal does no work and stays silent
    // otherwise (no upsell spam). The reader is tolerant — errors yield 0.
    let canon_pending = if signed_in {
        crate::commands::canon::suggestions::pending_total(ws)
    } else {
        0
    };

    Ok(EngineInputs {
        metrics,
        edits_since_annotation: state.edits_since_annotation,
        unreviewed_intents,
        proofs_awaiting_review,
        canon_pending,
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

// ─── hook emit (D10: the CLI nudges the AGENT; the agent talks to the human) ──

/// PostToolUse agent reminder: prod the coding agent to capture intent.
fn emit_agent_reminder(edits: usize) {
    println!("<system-reminder>");
    println!(
        "Aristo: {edits} source edits since your last annotation. If any of \
         them embodied a non-obvious decision (a chosen invariant, a \
         refactor trap, an intentional-not-incomplete choice), capture it now \
         with an `aristo::intent` while the rationale is fresh — see the \
         aristo-authoring skill. Skip if the edits were purely mechanical."
    );
    println!("</system-reminder>");
}

/// Stop consolidated nudge: a hook can't pop an AskUserQuestion (D10), so it
/// nudges the AGENT to offer the user the recommended review at a natural
/// pause. Subject-only — about the user's own annotations/verification.
fn emit_stop_reminder(decision: &Decision, inputs: &EngineInputs) {
    println!("<system-reminder>");
    println!("Aristo progress nudge — {}.", backlog_summary(inputs));
    if let Some(rec) = decision.recommended() {
        println!(
            "At a natural pause, offer the user: {}. Don't interrupt mid-task.",
            recommended_phrase(rec)
        );
    }
    println!("</system-reminder>");
}

/// SessionStart additionalContext: re-surface standing backlog so the agent
/// can offer a review early in the session. Same JSON shape the skills hook
/// uses.
fn emit_session_start_context(decision: &Decision, inputs: &EngineInputs) {
    let mut context = format!("Aristo: {}.", backlog_summary(inputs));
    if let Some(rec) = decision.recommended() {
        context.push_str(&format!(
            " When the user reaches a natural pause, offer: {}.",
            recommended_phrase(rec)
        ));
    }
    let json = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    });
    println!("{json}");
}

/// A subject-only one-line summary of the standing backlog.
fn backlog_summary(inputs: &EngineInputs) -> String {
    let mut parts = Vec::new();
    if inputs.unreviewed_intents > 0 {
        parts.push(format!(
            "{} intent(s) await review",
            inputs.unreviewed_intents
        ));
    }
    if inputs.canon_pending > 0 {
        parts.push(format!(
            "{} canon match(es)/suggestion(s) pending",
            inputs.canon_pending
        ));
    }
    if inputs.metrics.unverified > 0 {
        parts.push(format!(
            "{} of {} intents unverified",
            inputs.metrics.unverified, inputs.metrics.verifiable
        ));
    }
    if inputs.proofs_awaiting_review > 0 {
        parts.push(format!(
            "{} proof(s) await review",
            inputs.proofs_awaiting_review
        ));
    }
    if parts.is_empty() {
        format!(
            "tier {} (score {:.2})",
            inputs.metrics.tier.label(),
            inputs.metrics.visible_score
        )
    } else {
        parts.join(" · ")
    }
}

/// Map a signal id to the low-friction action the agent should offer.
fn recommended_phrase(signal_id: &str) -> &'static str {
    match signal_id {
        "congrats" => "a quick note on the progress just made (tier/score went up)",
        "review_backlog" => {
            "an intent review — Critique-first runs in the background while you continue"
        }
        "canon_pending" => "a look at the pending canon matches (aristo-intent-suggestions)",
        "verify_backlog" => "running `aristo verify` (can run in the background)",
        "proof_review_backlog" => "a review of the freshly-verified proofs",
        "score_slump" => "shoring up coverage — annotate or verify to recover the score",
        _ => "a review of the outstanding aristo items",
    }
}
