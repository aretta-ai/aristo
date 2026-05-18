//! `aristo session` — CLI surface for the generic review-session
//! substrate (slice 27.5, step 2).
//!
//! Each subcommand maps to one substrate operation in
//! `crate::session::*`. Per-kind side effects (e.g. mutating a
//! `.critique` file on accept) plug in via the `SessionKind` trait
//! wired in step 5; for step 2, `decide` updates only the substrate's
//! bucket state plus the rejection-log / backlog with empty per-kind
//! payloads.

use crate::session::{id_gen, storage, types};
use crate::workspace::Workspace;
use crate::{BucketArg, CliError, CliResult, SessionAction};

mod active;
mod decide;
mod exit;
mod list;
mod start;
mod status;

pub(crate) fn run(action: SessionAction) -> CliResult<()> {
    match action {
        SessionAction::Start {
            kind,
            subject,
            allow_nesting,
        } => start::run(&kind, &subject, allow_nesting),
        SessionAction::Active { hook_format } => active::run(hook_format),
        SessionAction::Status => status::run(),
        SessionAction::Decide { item, bucket, note } => decide::run(&item, bucket, note),
        SessionAction::Exit { defer_undecided } => exit::run_exit(defer_undecided),
        SessionAction::Abort { yes } => exit::run_abort(yes),
        SessionAction::List { limit } => list::run(limit),
    }
}

// ── Shared helpers for handlers ──────────────────────────────────────

pub(crate) fn workspace_or_error() -> CliResult<Workspace> {
    Workspace::find(None).map_err(|e| match e {
        crate::workspace::WorkspaceError::NotFound { searched_from } => {
            CliError::NotInWorkspace { searched_from }
        }
    })
}

/// Resolve the active session — load both the pointer and the session
/// file in one call. Returns `Ok(None)` when no session is active.
/// Stale pointers (pointer present, session file missing) are auto-
/// cleared so subsequent commands see no-active rather than perpetually
/// erroring.
pub(crate) fn load_active(ws: &Workspace) -> CliResult<Option<types::Session>> {
    match storage::read_active_pointer(ws)? {
        None => Ok(None),
        Some(id) => match storage::read_active_session(ws, &id)? {
            Some(s) => Ok(Some(s)),
            None => {
                storage::clear_active_pointer(ws)?;
                Ok(None)
            }
        },
    }
}

pub(crate) fn item_status_from_bucket(b: BucketArg) -> types::ItemStatus {
    match b {
        BucketArg::Accepted => types::ItemStatus::Accepted,
        BucketArg::Rejected => types::ItemStatus::Rejected,
        BucketArg::Pending => types::ItemStatus::Pending,
    }
}

/// Convenient wrapper around the substrate's RFC3339 formatter, sourced
/// from the system clock.
pub(crate) fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is post-1970");
    id_gen::format_rfc3339(now.as_secs())
}
