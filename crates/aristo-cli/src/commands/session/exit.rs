//! `aristo session exit` (+ `--defer-undecided`) and
//! `aristo session abort`.
//!
//! Three close paths:
//!
//! - **Strict exit** — errors out if any items are still open.
//! - **Defer-undecided** — moves still-open items to the per-kind
//!   backlog, then closes. Never silently drops.
//! - **Abort** — destructive: drops the session entirely with no
//!   decisions recorded. Requires `--yes` to skip confirmation.
//!
//! All three clear `.active` last; if any step before that fails the
//! pointer survives and the user can retry.

use std::io::{self, BufRead, Write};

use crate::session::backlog::BacklogEntry;
use crate::session::types::{ExitKind, ItemStatus, SessionState};
use crate::session::{backlog, storage};
use crate::{CliError, CliResult};

use super::{load_active, now_rfc3339, workspace_or_error};

pub(crate) fn run_exit(defer_undecided: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let Some(mut session) = load_active(&ws)? else {
        return Err(CliError::Other {
            message: "no active session".into(),
            exit_code: 1,
        });
    };

    let counts = session.bucket_counts();
    let now = now_rfc3339();

    if counts.open > 0 && !defer_undecided {
        return Err(CliError::Other {
            message: format!(
                "{} item(s) still open; pass `--defer-undecided` to move them to the backlog, \
                 or decide each via `aristo session decide --item <ref> --bucket <...>` first",
                counts.open
            ),
            exit_code: 1,
        });
    }

    if defer_undecided && counts.open > 0 {
        // Move every Open item to the per-kind backlog, then re-tag
        // its status as Pending in the session record (so the closed-
        // session audit trail reflects the final bucket).
        for item in session
            .items
            .iter_mut()
            .filter(|i| i.status == ItemStatus::Open)
        {
            backlog::append_entry(
                &ws,
                &session.kind,
                BacklogEntry {
                    item_ref: item.item_ref.clone(),
                    deferred_at: now.clone(),
                    deferred_from_session: session.id.clone(),
                    note: None,
                    data: serde_json::Value::Null,
                },
            )?;
            item.status = ItemStatus::Pending;
            item.closed_at = Some(now.clone());
        }
    }

    session.state = SessionState::Closed;
    session.closed_at = Some(now.clone());
    session.exit_kind = Some(if defer_undecided {
        ExitKind::ExitDeferUndecided
    } else {
        ExitKind::Exit
    });

    storage::write_active_session(&ws, &session)?;
    storage::move_to_closed(&ws, &session.id)?;
    storage::clear_active_pointer(&ws)?;

    let final_counts = session.bucket_counts();
    println!(
        "ok: closed session {} ({} accepted, {} rejected, {} pending)",
        session.id, final_counts.accepted, final_counts.rejected, final_counts.pending
    );
    Ok(())
}

pub(crate) fn run_abort(yes: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let Some(session) = load_active(&ws)? else {
        return Err(CliError::Other {
            message: "no active session".into(),
            exit_code: 1,
        });
    };

    if !yes && !confirm_abort(&session.id.to_string())? {
        return Err(CliError::Other {
            message: "abort cancelled".into(),
            exit_code: 1,
        });
    }

    let now = now_rfc3339();
    let mut closing = session.clone();
    closing.state = SessionState::Aborted;
    closing.closed_at = Some(now);
    closing.exit_kind = Some(ExitKind::Abort);

    storage::write_active_session(&ws, &closing)?;
    storage::move_to_closed(&ws, &closing.id)?;
    storage::clear_active_pointer(&ws)?;

    println!("ok: aborted session {}", closing.id);
    Ok(())
}

#[aristo::intent(
    "Abort prompts on stdin unless `--yes` is given. The default-no \
     posture matches every other destructive aristo command (no \
     `aristo stamp --force` without explicit opt-in). A refactor that \
     defaulted to yes would silently drop a session's audit trail on \
     any typo'd subcommand.",
    verify = "neural",
    id = "abort_requires_explicit_confirmation"
)]
fn confirm_abort(id: &str) -> CliResult<bool> {
    print!("abort session {id}? this drops all decisions. [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
}
