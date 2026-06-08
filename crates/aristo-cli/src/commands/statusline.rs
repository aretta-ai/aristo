//! `aristo statusline` — a compact ambient status segment for the Claude Code
//! `statusLine` (Phase 18 #9, nudge surface v2). Prints one line like
//! `aristo  3 review · 2 unverified · Apprentice` for the terminal status bar.
//!
//! This is the USER-facing ambient surface (the agent-facing nudges ride the
//! PostToolUse / UserPromptSubmit hooks). It is read-only and stays cheap: it
//! reads the index (counts) + the git-untracked nudge-state (reviewed map +
//! the cached tier from the edit-window baseline) and does NOT walk source —
//! the status bar re-renders constantly, so a per-render source walk would be
//! too expensive. The tier is therefore "as of the session baseline".

use std::io::Read;
use std::path::PathBuf;

use aristo_core::index::VerifyLevel;

use crate::commands::show::read_index;
use crate::nudge::intents::authored_intents;
use crate::nudge::state::{NudgeState, STATE_FILENAME};
use crate::{CliResult, Workspace};

#[aristo::intent(
    "The statusline is read-only and TOLERANT: on any failure — no workspace, \
     an unreadable index, or nudges globally off — it prints nothing and exits \
     0. The status bar re-renders on every turn, so a statusline that errored \
     or wrote files would corrupt the bar or thrash the workspace on every \
     keystroke. Silence is the correct degraded state.",
    verify = "test",
    id = "statusline_is_read_only_and_tolerant"
)]
pub(crate) fn run() -> CliResult<()> {
    // The Claude Code statusLine command receives a JSON payload on stdin with
    // the session's cwd; fall back to the process cwd.
    let start = cwd_from_stdin();
    let Ok(ws) = Workspace::find(start.as_deref()) else {
        return Ok(()); // not an aristo workspace → empty segment
    };
    if ws.load_config().nudges.aggressiveness.is_off() {
        return Ok(()); // honor the global opt-out
    }
    let Ok(index) = read_index(&ws.index_path()) else {
        return Ok(());
    };
    let state = NudgeState::load(&ws.aristo_dir().join(STATE_FILENAME));
    let intents = authored_intents(&index);

    let unreviewed = state.unreviewed_count(
        intents
            .iter()
            .map(|i| (i.id.as_str(), i.text_hash.as_str(), i.body_hash.as_str())),
    );
    let unverified = intents
        .iter()
        .filter(|i| !matches!(i.verify, VerifyLevel::Bool(false)) && !i.status.is_terminal_clean())
        .count();
    // Cached tier from the edit-window baseline (cheap — no source walk).
    let tier = state.baseline.as_ref().map(|b| b.tier.clone());

    if let Some(line) = statusline(unreviewed, unverified, tier.as_deref()) {
        println!("{line}");
    }
    Ok(())
}

/// Build the compact status segment, or `None` when there is nothing to show
/// (no backlog and no known tier → no aristo segment in the bar).
fn statusline(unreviewed: usize, unverified: usize, tier: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if unreviewed > 0 {
        parts.push(format!("{unreviewed} review"));
    }
    if unverified > 0 {
        parts.push(format!("{unverified} unverified"));
    }
    if let Some(t) = tier {
        parts.push(t.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("aristo  {}", parts.join(" · ")))
    }
}

/// Extract the session cwd from the statusLine stdin payload, if present.
fn cwd_from_stdin() -> Option<PathBuf> {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() || buf.trim().is_empty() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_str(&buf).ok()?;
    let dir = json
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(|v| v.as_str())
        .or_else(|| json.get("cwd").and_then(|v| v.as_str()))?;
    Some(PathBuf::from(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_when_nothing_to_surface() {
        assert_eq!(statusline(0, 0, None), None);
    }

    #[test]
    fn shows_backlog_and_tier_compactly() {
        assert_eq!(
            statusline(3, 2, Some("Apprentice")).as_deref(),
            Some("aristo  3 review · 2 unverified · Apprentice")
        );
    }

    #[test]
    fn omits_zero_counts_but_keeps_tier() {
        assert_eq!(
            statusline(0, 0, Some("Adept")).as_deref(),
            Some("aristo  Adept")
        );
        assert_eq!(statusline(5, 0, None).as_deref(), Some("aristo  5 review"));
    }
}
