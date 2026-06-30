//! `aristo statusline` — ambient progress + staleness segment for the Claude
//! Code `statusLine` (Phase 18, statusline v2). Prints one line such as
//! `aristo  Apprentice · 3 review · 12/14 verified · ⚠ 2 stale`, or, while a
//! review session is open, `aristo  ⏸ intent-review 2/7 · stamp/verify paused`.
//!
//! This is the USER-facing ambient surface (the agent-facing nudges ride the
//! PostToolUse / UserPromptSubmit hooks). It is read-only and stays cheap: it
//! reads the index (counts + proof-sourced status), the git-untracked
//! nudge-state (reviewed map + the cached baseline tier), the active-session
//! pointer, the config, and a local sign-in check.
//! It does NOT walk/parse source — the status bar re-renders constantly, so a
//! per-render source walk would be too expensive; the tier is therefore the
//! cached session-baseline label. Subject-only: every token is about the
//! user's own annotations / verification, never the internal model.

use std::io::Read;
use std::path::PathBuf;

use aristo_core::index::{Status, VerifyLevel};

use crate::commands::show::read_index;
use crate::nudge::intents::authored_intents;
use crate::nudge::state::{NudgeState, STATE_FILENAME};
use crate::{CliResult, Workspace};

/// A one-line summary of the active review session, when one is open.
struct SessionView {
    /// The pipeline kind, e.g. `critique-review` / `proof-review` / `intent-review`.
    kind: String,
    /// Items still awaiting a decision.
    open: usize,
    /// Total items in the session (open + decided).
    total: usize,
}

/// Everything the pure renderer needs, filled by [`run`] from cheap reads.
#[derive(Default)]
struct BarView {
    /// Cached session-baseline tier label (may be stale mid-session).
    tier: Option<String>,
    /// Unreviewed authored intents (remaining).
    unreviewed: usize,
    /// Verifiable intents in a terminal-clean status.
    verified_clean: usize,
    /// Verifiable intents (verify != false): the coverage denominator.
    verifiable: usize,
    /// Intents whose proof is broken or drift-suspected (staleness warning).
    stale: usize,
    /// Pending canon matches/suggestions (signed-in only; 0 otherwise).
    canon: usize,
    /// The active review session, if one is open (takes over the bar).
    session: Option<SessionView>,
}

#[aristo::intent(
    "The statusline is read-only and TOLERANT: on any failure — no workspace, \
     an unreadable index, or nudges globally off — it prints nothing and exits \
     0. The status bar re-renders on every turn, so a statusline that errored \
     or wrote files would corrupt the bar or thrash the workspace on every \
     keystroke. Silence is the correct degraded state.",
    verify = "test",
    id = "statusline_is_read_only_and_tolerant"
)]
#[aristo::intent(
    "Each render reads a bounded set — the index, the nudge-state file, the \
     active-session pointer, and a local sign-in check — and never walks or \
     stats the source tree. The bar re-renders on every keystroke, so adding a \
     source-tree walk or a per-file stat here would silently make the prompt \
     slow on every render. The tier shown is the cached session baseline for \
     exactly this reason, not a freshly-measured one: intentional, not \
     incomplete.",
    verify = "neural",
    id = "statusline_render_reads_are_bounded"
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

    let mut view = BarView::default();

    // Cached baseline tier (cheap; possibly stale until a metrics cache lands).
    let state = NudgeState::load(&ws.aristo_dir().join(STATE_FILENAME));
    view.tier = state.baseline.as_ref().map(|b| b.tier.clone());

    // An active review session takes over the bar — the D11 guard has paused
    // stamp/verify, so the standing backlog counts would mislead.
    view.session = active_session_view(&ws);

    if view.session.is_none() {
        if let Ok(index) = read_index(&ws.index_path()) {
            let intents = authored_intents(&index);
            view.unreviewed = state.unreviewed_count(
                intents
                    .iter()
                    .map(|i| (i.id.as_str(), i.text_hash.as_str(), i.body_hash.as_str())),
            );
            for i in &intents {
                if matches!(i.verify, VerifyLevel::Bool(false)) {
                    continue; // documentation-only: never verifiable
                }
                view.verifiable += 1;
                if i.status.is_terminal_clean() {
                    view.verified_clean += 1;
                }
                if intent_is_stale(i.status) {
                    view.stale += 1;
                }
            }
        }
    }

    // Canon is paid (#10): only ever shown signed in (never an upsell when out).
    if aristo_core::auth::resolve_full().is_ok() {
        view.canon = crate::commands::canon::suggestions::pending_total(&ws);
    }

    if let Some(line) = statusline(&view) {
        let color = std::env::var_os("NO_COLOR").is_none();
        println!("{}", colorize(&line, &view, color));
    }
    Ok(())
}

/// Build the compact status segment in PLAIN text (color is layered on top by
/// [`colorize`]), or `None` when there is nothing worth showing.
fn statusline(view: &BarView) -> Option<String> {
    // Active session: lead with the resume cue; it replaces the (guard-paused)
    // backlog counts. Tier + canon may trail.
    if let Some(s) = &view.session {
        let mut parts = vec![
            format!("⏸ {} {}/{}", s.kind, s.open, s.total),
            "stamp/verify paused".to_string(),
        ];
        if let Some(t) = &view.tier {
            parts.push(t.clone());
        }
        if view.canon > 0 {
            parts.push(format!("{} canon", view.canon));
        }
        return Some(format!("aristo  {}", parts.join(" · ")));
    }

    // Clean + complete: collapse to the tier + a green check.
    let complete = view.unreviewed == 0
        && view.stale == 0
        && view.verifiable > 0
        && view.verified_clean == view.verifiable;
    if complete {
        if let Some(t) = &view.tier {
            return Some(format!("aristo  {t} ✓"));
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = &view.tier {
        parts.push(t.clone());
    }
    if view.unreviewed > 0 {
        parts.push(format!("{} review", view.unreviewed));
    }
    if view.verifiable > 0 {
        parts.push(format!(
            "{}/{} verified",
            view.verified_clean, view.verifiable
        ));
    }
    if view.stale > 0 {
        parts.push(format!("⚠ {} stale", view.stale));
    }
    if view.canon > 0 {
        parts.push(format!("{} canon", view.canon));
    }

    if parts.is_empty() {
        None
    } else {
        Some(format!("aristo  {}", parts.join(" · ")))
    }
}

/// Layer ANSI color onto the plain bar. Decoration only: every glyph/word is
/// meaningful without it, so `color = false` (or `NO_COLOR`) returns the plain
/// string unchanged. Green for success (`✓` / fully-verified coverage), amber
/// for the staleness warning, bold for the session resume cue.
fn colorize(line: &str, view: &BarView, color: bool) -> String {
    if !color {
        return line.to_string();
    }
    let mut s = line.to_string();
    if view.stale > 0 {
        let tok = format!("⚠ {} stale", view.stale);
        s = s.replace(&tok, &format!("\x1b[33m{tok}\x1b[0m"));
    }
    s = s.replace(" ✓", " \x1b[32m✓\x1b[0m");
    if view.session.is_none() && view.verifiable > 0 && view.verified_clean == view.verifiable {
        let tok = format!("{}/{} verified", view.verified_clean, view.verifiable);
        s = s.replace(&tok, &format!("\x1b[32m{tok}\x1b[0m"));
    }
    if view.session.is_some() {
        s = s.replace('⏸', "\x1b[1m⏸");
        s = s.replace(" · stamp/verify paused", "\x1b[0m · stamp/verify paused");
    }
    s
}

/// Read the active review session (if any) as a compact view. Cheap: a single
/// pointer read + one small TOML parse, no index or source walk. Tolerant: a
/// stale pointer (file missing) reads as "no session".
fn active_session_view(ws: &Workspace) -> Option<SessionView> {
    let id = crate::session::storage::read_active_pointer(ws)
        .ok()
        .flatten()?;
    let session = crate::session::storage::read_active_session(ws, &id)
        .ok()
        .flatten()?;
    let c = session.bucket_counts();
    let total = c.open + c.accepted + c.rejected + c.pending;
    Some(SessionView {
        kind: session.kind.clone(),
        open: c.open,
        total,
    })
}

#[aristo::intent(
    "The bar's staleness count comes from the proofs-join status, not a \
     file-mtime heuristic: an intent is stale iff it is recorded broken — \
     Status::Stale (code drifted from its proof) or Counterexample (refuted). \
     Unknown/Inconclusive are unverified, not stale. This needs no per-render \
     source parse and no index mtime, so the bar stays cheap, and it can't \
     disagree with `aristo status` on what 'stale' means.",
    verify = "neural",
    id = "statusline_staleness_is_from_proof_status"
)]
fn intent_is_stale(status: Status) -> bool {
    matches!(status, Status::Stale | Status::Counterexample)
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

    fn view() -> BarView {
        BarView::default()
    }

    #[test]
    fn empty_when_nothing_to_surface() {
        assert_eq!(statusline(&view()), None);
    }

    #[test]
    fn clean_and_complete_collapses_to_tier_check() {
        let v = BarView {
            tier: Some("Adept".into()),
            verifiable: 14,
            verified_clean: 14,
            ..view()
        };
        assert_eq!(statusline(&v).as_deref(), Some("aristo  Adept ✓"));
    }

    #[test]
    fn typical_shows_review_and_coverage() {
        let v = BarView {
            tier: Some("Apprentice".into()),
            unreviewed: 3,
            verifiable: 14,
            verified_clean: 12,
            ..view()
        };
        assert_eq!(
            statusline(&v).as_deref(),
            Some("aristo  Apprentice · 3 review · 12/14 verified")
        );
    }

    #[test]
    fn drift_shows_amber_stale_token() {
        let v = BarView {
            tier: Some("Apprentice".into()),
            unreviewed: 3,
            verifiable: 14,
            verified_clean: 12,
            stale: 3,
            ..view()
        };
        assert_eq!(
            statusline(&v).as_deref(),
            Some("aristo  Apprentice · 3 review · 12/14 verified · ⚠ 3 stale")
        );
    }

    #[test]
    fn canon_shows_only_when_present_signed_in() {
        let v = BarView {
            tier: Some("Apprentice".into()),
            verifiable: 14,
            verified_clean: 12,
            canon: 4,
            ..view()
        };
        assert_eq!(
            statusline(&v).as_deref(),
            Some("aristo  Apprentice · 12/14 verified · 4 canon")
        );
        // Signed-out (canon = 0) drops the token entirely.
        let v0 = BarView { canon: 0, ..v };
        assert_eq!(
            statusline(&v0).as_deref(),
            Some("aristo  Apprentice · 12/14 verified")
        );
    }

    #[test]
    fn active_session_takes_over_and_suppresses_counts() {
        let v = BarView {
            tier: Some("Apprentice".into()),
            unreviewed: 3,
            verifiable: 14,
            verified_clean: 12,
            stale: 5,
            session: Some(SessionView {
                kind: "intent-review".into(),
                open: 2,
                total: 7,
            }),
            ..view()
        };
        assert_eq!(
            statusline(&v).as_deref(),
            Some("aristo  ⏸ intent-review 2/7 · stamp/verify paused · Apprentice")
        );
    }

    #[test]
    fn no_tier_still_shows_work() {
        let v = BarView {
            unreviewed: 2,
            ..view()
        };
        assert_eq!(statusline(&v).as_deref(), Some("aristo  2 review"));
    }

    #[test]
    fn color_is_decoration_and_degrades_to_plain() {
        let v = BarView {
            tier: Some("Apprentice".into()),
            verifiable: 14,
            verified_clean: 12,
            stale: 3,
            ..view()
        };
        let plain = statusline(&v).unwrap();
        assert_eq!(colorize(&plain, &v, false), plain);
        let colored = colorize(&plain, &v, true);
        assert!(colored.contains("⚠ 3 stale"));
        assert!(colored.contains("\x1b[33m"));
        assert!(colored.contains("\x1b[0m"));
    }

    #[test]
    fn stale_logic_is_recorded_broken_only() {
        // Stale comes from the proofs-join status: recorded broken only.
        assert!(intent_is_stale(Status::Stale));
        assert!(intent_is_stale(Status::Counterexample));
        // Terminal-clean is fresh; the mtime drift heuristic is retired.
        assert!(!intent_is_stale(Status::Verified));
        assert!(!intent_is_stale(Status::Tested));
        assert!(!intent_is_stale(Status::Neural));
        // Unverified states are not stale.
        assert!(!intent_is_stale(Status::Unknown));
        assert!(!intent_is_stale(Status::Inconclusive));
    }
}
