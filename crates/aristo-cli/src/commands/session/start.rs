//! `aristo session start <kind> --subject <focal-ref>`.
//!
//! Creates a fresh session. Refuses if a session is already active.

use crate::session::{id_gen, storage, types};
use crate::{CliError, CliResult};

use super::{load_active, now_rfc3339, workspace_or_error};

pub(crate) fn run(kind: &str, subject: &str, allow_nesting: bool) -> CliResult<()> {
    let _ = allow_nesting; // v0 ships Disallow only; flag reserved for future per-kind opt-ins (design Q4)
    let ws = workspace_or_error()?;

    if let Some(existing) = load_active(&ws)? {
        return Err(CliError::Other {
            message: format!(
                "a session is already active (id={}, kind={}, subject={}).\n\
                 exit it first with `aristo session exit` or \
                 `aristo session exit --defer-undecided`, or use \
                 `aristo session abort` to drop it entirely.",
                existing.id, existing.kind, existing.subject
            ),
            exit_code: 1,
        });
    }

    let id = id_gen::mint_session_id();
    let session = types::Session {
        schema_version: 1,
        id: id.clone(),
        kind: kind.to_string(),
        subject: subject.to_string(),
        started_at: now_rfc3339(),
        started_by: "cli".into(),
        nesting_policy: types::NestingPolicy::Disallow,
        state: types::SessionState::Active,
        items: Vec::new(),
        closed_at: None,
        exit_kind: None,
    };
    storage::write_active_session(&ws, &session)?;
    storage::write_active_pointer(&ws, &id)?;

    println!("ok: started session {id} (kind={kind}, subject={subject})");
    Ok(())
}
