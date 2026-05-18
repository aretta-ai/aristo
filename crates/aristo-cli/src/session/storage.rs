//! On-disk storage for sessions and the `.active` pointer.
//!
//! Layout (gitignored):
//!
//! ```text
//! .aristo/sessions/
//!   .active                       # pointer to active session id (single line, no trailing newline)
//!   active/
//!     <id>.session.toml           # in-flight session state
//!   closed/
//!     <id>.session.toml           # closed-session audit trail
//! ```
//!
//! There is at most one active session per workspace — the `.active`
//! pointer's existence is the single source of truth for "is anything
//! active." All writes go through [`atomic_write`] (temp-file +
//! rename) so concurrent reads never see partial state.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use crate::commands::index::atomic_write;
use crate::session::types::{Session, SessionId};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// Per-session filename convention. Kind lives inside the TOML body;
/// the filename is `<id>.session.toml` to keep the path stable across
/// any future renames of the kind.
fn session_filename(id: &SessionId) -> String {
    format!("{}.session.toml", id.as_str())
}

/// Path to an active session's TOML file.
pub fn active_session_path(ws: &Workspace, id: &SessionId) -> PathBuf {
    ws.sessions_active_session_dir().join(session_filename(id))
}

/// Path to a closed session's TOML file.
pub fn closed_session_path(ws: &Workspace, id: &SessionId) -> PathBuf {
    ws.sessions_closed_dir().join(session_filename(id))
}

/// Create the session directories if missing. Idempotent.
pub fn ensure_dirs(ws: &Workspace) -> CliResult<()> {
    fs::create_dir_all(ws.sessions_active_session_dir())?;
    fs::create_dir_all(ws.sessions_closed_dir())?;
    fs::create_dir_all(ws.sessions_backlog_dir())?;
    Ok(())
}

#[aristo::intent(
    "Session writes go through `atomic_write` (temp-file + rename) so a \
     concurrent reader cannot observe a partially-serialized session \
     file. The session TOML is the single source of truth for in-flight \
     state; a half-written file would deserialize-error and look like \
     'session is gone' from the reader's perspective. A refactor that \
     used `fs::write` directly would re-introduce the partial-read \
     window between open and close.",
    verify = "neural",
    id = "session_writes_are_atomic_via_tempfile_rename"
)]
pub fn write_active_session(ws: &Workspace, session: &Session) -> CliResult<()> {
    ensure_dirs(ws)?;
    let path = active_session_path(ws, &session.id);
    let toml_text = toml::to_string(session).map_err(|e| CliError::Other {
        message: format!("session serialize: {e}"),
        exit_code: 1,
    })?;
    atomic_write(&path, &toml_text)
}

/// Read an active session by id. Returns `Ok(None)` if no such file
/// exists (the session was closed, aborted, or never created).
pub fn read_active_session(ws: &Workspace, id: &SessionId) -> CliResult<Option<Session>> {
    let path = active_session_path(ws, id);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let session = toml::from_str(&text).map_err(|e| CliError::Other {
        message: format!("session parse {}: {e}", path.display()),
        exit_code: 1,
    })?;
    Ok(Some(session))
}

#[aristo::intent(
    "Closing a session is an atomic file move from active/ to closed/. \
     A reader that sees the file is gone from active/ MUST find it in \
     closed/ (or .active was cleared first — see \
     `[[clear_active_pointer]]`). A refactor that copied + deleted \
     instead of renaming would introduce a window where the session \
     exists in both directories (stale-read risk) or in neither (lost \
     audit trail).",
    verify = "neural",
    id = "session_close_uses_atomic_rename_active_to_closed"
)]
pub fn move_to_closed(ws: &Workspace, id: &SessionId) -> CliResult<()> {
    ensure_dirs(ws)?;
    let src = active_session_path(ws, id);
    let dst = closed_session_path(ws, id);
    fs::rename(&src, &dst).map_err(Into::into)
}

/// Read the `.active` pointer. Returns `Ok(None)` if no active session.
#[aristo::intent(
    "`.active` is the single source of truth for 'is there an active \
     session?'. Reading it is the first step of every SDK pre-check. \
     Whitespace-trim the contents so a stray trailing newline (from an \
     editor or a shell `echo`) doesn't break id lookup. A refactor that \
     used the contents verbatim would silently treat an edited pointer \
     as no-such-session.",
    verify = "neural",
    id = "active_pointer_read_trims_whitespace"
)]
pub fn read_active_pointer(ws: &Workspace) -> CliResult<Option<SessionId>> {
    let path = ws.sessions_active_pointer();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(SessionId::from_string(trimmed.to_string())))
}

/// Atomically set the `.active` pointer to `id`. Overwrites any prior
/// value — callers enforce single-active semantics by checking
/// `read_active_pointer` first.
pub fn write_active_pointer(ws: &Workspace, id: &SessionId) -> CliResult<()> {
    ensure_dirs(ws)?;
    atomic_write(&ws.sessions_active_pointer(), id.as_str())
}

#[aristo::intent(
    "Clearing `.active` is the last step of every session-exit flow \
     (strict exit, defer-undecided exit, and abort all clear it). A \
     refactor that left `.active` pointing at a closed session would \
     break every subsequent pre-check — the SDK would think a session \
     is in flight that the user already exited. Idempotent (missing \
     file is fine) so re-running an exit handler doesn't error.",
    verify = "neural",
    id = "clear_active_pointer_is_idempotent_and_load_bearing_for_exit"
)]
pub fn clear_active_pointer(ws: &Workspace) -> CliResult<()> {
    let path = ws.sessions_active_pointer();
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{ExitKind, Item, ItemRef, NestingPolicy, Session, SessionState};
    use tempfile::TempDir;

    fn fresh_workspace(dir: &TempDir) -> Workspace {
        std::fs::write(dir.path().join("aristo.toml"), "").unwrap();
        Workspace {
            root: dir.path().to_path_buf(),
        }
    }

    fn fixture_session(id_text: &str) -> Session {
        Session {
            schema_version: 1,
            id: SessionId::from_string(id_text.into()),
            kind: "critique-review".into(),
            subject: "src/foo.rs".into(),
            started_at: "2026-05-18T13:00:00Z".into(),
            started_by: "test".into(),
            nesting_policy: NestingPolicy::Disallow,
            state: SessionState::Active,
            items: vec![Item::open(ItemRef::new("foo", 0))],
            closed_at: None,
            exit_kind: None,
        }
    }

    #[test]
    fn write_then_read_active_session_round_trips() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let s = fixture_session("01J5K9N7");
        write_active_session(&ws, &s).unwrap();
        let back = read_active_session(&ws, &s.id).unwrap().unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn read_active_session_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let id = SessionId::from_string("nope".into());
        assert_eq!(read_active_session(&ws, &id).unwrap(), None);
    }

    #[test]
    fn move_to_closed_relocates_the_file() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let mut s = fixture_session("01J5K9N7");
        write_active_session(&ws, &s).unwrap();
        // Caller flips state + stamps exit_kind before moving (closed
        // sessions are immutable per design).
        s.state = SessionState::Closed;
        s.exit_kind = Some(ExitKind::Exit);
        s.closed_at = Some("2026-05-18T13:30:00Z".into());
        write_active_session(&ws, &s).unwrap();
        move_to_closed(&ws, &s.id).unwrap();
        assert_eq!(read_active_session(&ws, &s.id).unwrap(), None);
        assert!(closed_session_path(&ws, &s.id).is_file());
    }

    #[test]
    fn active_pointer_round_trip() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        assert_eq!(read_active_pointer(&ws).unwrap(), None);
        let id = SessionId::from_string("01J5K9N7".into());
        write_active_pointer(&ws, &id).unwrap();
        assert_eq!(read_active_pointer(&ws).unwrap(), Some(id.clone()));
        clear_active_pointer(&ws).unwrap();
        assert_eq!(read_active_pointer(&ws).unwrap(), None);
    }

    #[test]
    fn active_pointer_trims_whitespace_and_treats_empty_as_none() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        ensure_dirs(&ws).unwrap();
        // Simulate an editor that added a trailing newline.
        std::fs::write(ws.sessions_active_pointer(), "01J5K9N7\n").unwrap();
        let id = read_active_pointer(&ws).unwrap().unwrap();
        assert_eq!(id.as_str(), "01J5K9N7");
        // Whitespace-only contents = no active session.
        std::fs::write(ws.sessions_active_pointer(), "   \n").unwrap();
        assert_eq!(read_active_pointer(&ws).unwrap(), None);
    }

    #[test]
    fn clear_active_pointer_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        // No active pointer exists yet — clearing must succeed silently.
        clear_active_pointer(&ws).unwrap();
        // Write + clear + clear again.
        write_active_pointer(&ws, &SessionId::from_string("x".into())).unwrap();
        clear_active_pointer(&ws).unwrap();
        clear_active_pointer(&ws).unwrap();
    }
}
