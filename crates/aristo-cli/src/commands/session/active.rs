//! `aristo session active [--hook-format]`.
//!
//! Prints the active session id (or empty stdout if none). With
//! `--hook-format`, prints the full `<system-reminder>` block the
//! `UserPromptSubmit` Claude Code hook injects every turn — see
//! design doc D2 layer 2.

use crate::workspace::Workspace;
use crate::{CliError, CliResult};

use super::load_active;

#[aristo::intent(
    "`aristo session active` is wired into Claude Code's \
     UserPromptSubmit hook (Layer 2). The hook fires on EVERY prompt \
     across EVERY project, not only aristo workspaces. So `active` \
     must exit 0 with empty stdout when run outside an aristo \
     workspace — a hard error would block every prompt in any \
     non-aristo project the user opens. The interactive form (no \
     `--hook-format`) follows the same rule for symmetry: an active \
     session can only exist within a workspace anyway, so no-workspace \
     and no-session are observationally identical at the CLI surface.",
    verify = "neural",
    id = "session_active_is_noop_outside_workspace"
)]
pub(crate) fn run(hook_format: bool) -> CliResult<()> {
    // No workspace → no session possible → empty stdout, exit 0.
    // Hook-friendly: works in any directory.
    let Ok(ws) = Workspace::find(None) else {
        return Ok(());
    };
    // Distinguish a real I/O error (which we still want to surface)
    // from "no active session" (which we don't).
    let session = match load_active(&ws) {
        Ok(s) => s,
        Err(CliError::NotInWorkspace { .. }) => return Ok(()),
        Err(e) => return Err(e),
    };

    if hook_format {
        emit_hook_block(session.as_ref());
    } else if let Some(s) = session {
        println!("{}", s.id);
    }
    // Empty stdout when no session — exit 0 either way.
    Ok(())
}

/// Emit the kind-agnostic system-reminder block. Stays empty when
/// no session is active so the hook injects nothing in that case
/// (the hook calls this command unconditionally; the SDK is the one
/// that knows whether to render).
fn emit_hook_block(session: Option<&crate::session::types::Session>) {
    let Some(s) = session else {
        return;
    };
    let counts = s.bucket_counts();
    // The exact wording is part of the substrate's contract with the
    // hook — see `docs/decisions/review-sessions.md` §D2 layer 2.
    println!("<system-reminder>");
    println!("You are currently in an aristo review session:");
    println!("  id:      {}", s.id);
    println!("  kind:    {}", s.kind);
    println!("  subject: {}", s.subject);
    println!(
        "  items:   {} open, {} accepted, {} rejected, {} pending",
        counts.open, counts.accepted, counts.rejected, counts.pending
    );
    println!();
    println!("While this session is active:");
    println!("  - You cannot start a different review session, run aristo");
    println!("    mutation commands, or move to planning/implementation.");
    println!("  - You may: continue this review, run read-only aristo commands,");
    println!("    have discussion with the user.");
    println!("  - Direct file edits via Edit are allowed but discouraged");
    println!("    (any unrelated change should wait for clean exit).");
    println!();
    println!("Commands:");
    println!("  aristo session status                       # peek state");
    println!("  aristo session decide --item <ref> --bucket <accepted|rejected|pending>");
    println!("  aristo session exit                         # strict close (all items decided)");
    println!("  aristo session exit --defer-undecided       # close, move open items to pending");
    println!("  aristo session abort                        # destructive cancel");
    println!("</system-reminder>");
}
