//! SDK pre-check guards (slice 27.5, step 3).
//!
//! Layer 1 of the three-layer enforcement described in
//! `docs/decisions/review-sessions.md` D2: at the top of every
//! mutating `aristo` command, call [`ensure_no_active_session`] to
//! refuse the operation when a review session is in flight.
//!
//! Layer 2 (Claude Code hook) and Layer 3 (skill-body discipline)
//! provide complementary nudges — the hook injects a system-reminder
//! every turn so the agent can't forget the session is open, and the
//! skill bodies check at step 1. Together: agent can't drift; SDK
//! refuses if it tries.
//!
//! The guard intentionally only knows "is there an active session";
//! it does NOT know which command it's gating. Per-command messages
//! are passed in by the caller so the user sees an actionable hint
//! ("exit your active critique-review first to run `aristo stamp`")
//! rather than a generic refusal.

use crate::session::storage;
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

#[aristo::intent(
    "Every mutating aristo command MUST call `ensure_no_active_session` \
     before touching shared state (index, proof/critique files, source). \
     This is Layer 1 of three (hook + skill-body discipline are the \
     other two). Layer 1 is the only one that's mechanically \
     enforceable — the hook is advisory and the skill body is \
     documentation. A refactor that bypasses the guard for any \
     mutating command re-introduces the slop-drift failure mode the \
     substrate exists to prevent: artifacts get committed without the \
     user noticing they bypassed review.",
    verify = "neural",
    id = "every_mutating_command_calls_ensure_no_active_session"
)]
pub fn ensure_no_active_session(ws: &Workspace, attempted: &str) -> CliResult<()> {
    let Some(id) = storage::read_active_pointer(ws)? else {
        return Ok(());
    };
    // Stale pointer → no active session in practice. Match
    // load_active's stale-pointer recovery so the guard doesn't
    // perpetually refuse on a dangling pointer.
    let Some(session) = storage::read_active_session(ws, &id)? else {
        storage::clear_active_pointer(ws)?;
        return Ok(());
    };

    Err(CliError::Other {
        message: format!(
            "active review session blocks `{attempted}`.\n\
             session: id={} kind={} subject={}\n\
             exit the session first with `aristo session exit` \
             (or `aristo session exit --defer-undecided` to park open \
             items), then retry.\n\
             see `aristo session status` for the current bucket counts.",
            session.id, session.kind, session.subject
        ),
        exit_code: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{
        Item, ItemRef, ItemStatus, NestingPolicy, Session, SessionId, SessionState,
    };
    use tempfile::TempDir;

    fn fresh_workspace(dir: &TempDir) -> Workspace {
        std::fs::write(dir.path().join("aristo.toml"), "").unwrap();
        Workspace {
            root: dir.path().to_path_buf(),
        }
    }

    fn write_session(ws: &Workspace, id: &str, kind: &str) {
        let s = Session {
            schema_version: 1,
            id: SessionId::from_string(id.into()),
            kind: kind.into(),
            subject: "test".into(),
            started_at: "2026-05-18T13:00:00Z".into(),
            started_by: "test".into(),
            nesting_policy: NestingPolicy::Disallow,
            state: SessionState::Active,
            items: vec![Item {
                item_ref: ItemRef::from_opaque("x#0"),
                status: ItemStatus::Open,
                note: None,
                closed_at: None,
            }],
            closed_at: None,
            exit_kind: None,
        };
        storage::write_active_session(ws, &s).unwrap();
        storage::write_active_pointer(ws, &s.id).unwrap();
    }

    #[test]
    fn ok_when_no_active_session() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        ensure_no_active_session(&ws, "aristo stamp").unwrap();
    }

    #[test]
    fn refuses_when_session_active_with_actionable_message() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        write_session(&ws, "01J5K9N7", "critique-review");
        let err = ensure_no_active_session(&ws, "aristo stamp").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("aristo stamp"), "msg: {msg}");
        assert!(msg.contains("critique-review"), "msg: {msg}");
        assert!(msg.contains("aristo session exit"), "msg: {msg}");
    }

    #[test]
    fn stale_pointer_clears_and_returns_ok() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        // Pointer to a session id whose file doesn't exist.
        std::fs::create_dir_all(ws.sessions_dir()).unwrap();
        std::fs::write(ws.sessions_active_pointer(), "ghost").unwrap();
        ensure_no_active_session(&ws, "aristo stamp").unwrap();
        // Guard should have cleared the stale pointer.
        assert!(storage::read_active_pointer(&ws).unwrap().is_none());
    }
}
