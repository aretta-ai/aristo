//! `aristo session list [--limit N]` — active session + N most-recent
//! closed sessions.
//!
//! Ordering relies on the [`id_gen::mint_session_id`]'s timestamp
//! prefix: filenames sort chronologically, so the newest closed
//! session is the last entry from `fs::read_dir`. We collect, sort,
//! take the last N.

use std::fs;

use crate::session::storage;
use crate::session::types::SessionId;
use crate::{CliError, CliResult};

use super::{load_active, workspace_or_error};

pub(crate) fn run(limit: usize) -> CliResult<()> {
    let ws = workspace_or_error()?;

    if let Some(s) = load_active(&ws)? {
        let counts = s.bucket_counts();
        println!(
            "active: {}  kind={}  subject={}  ({} open, {} accepted, {} rejected, {} pending)",
            s.id, s.kind, s.subject, counts.open, counts.accepted, counts.rejected, counts.pending
        );
    } else {
        println!("active: (none)");
    }

    let closed_dir = ws.sessions_closed_dir();
    let mut closed_ids = match fs::read_dir(&closed_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".session.toml"))
                    .map(SessionId::from)
            })
            .collect::<Vec<_>>(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(CliError::Io(e)),
    };
    // Lex sort = chronological sort (id_gen guarantees this).
    closed_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    println!();
    println!("closed (most recent {limit}):");
    let recent: Vec<_> = closed_ids.iter().rev().take(limit).collect();
    if recent.is_empty() {
        println!("  (none)");
        return Ok(());
    }
    for id in recent {
        let path = storage::closed_session_path(&ws, id);
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<crate::session::types::Session>(&text) {
                Ok(s) => {
                    let counts = s.bucket_counts();
                    println!(
                        "  {}  kind={}  exit={:?}  ({} accepted, {} rejected, {} pending)",
                        s.id,
                        s.kind,
                        s.exit_kind.unwrap_or(crate::session::types::ExitKind::Exit),
                        counts.accepted,
                        counts.rejected,
                        counts.pending
                    );
                }
                Err(_) => println!("  {id}  (parse error)"),
            },
            Err(_) => println!("  {id}  (read error)"),
        }
    }
    Ok(())
}
