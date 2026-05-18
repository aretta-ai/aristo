//! `aristo session status` — bucket counts + open items for the
//! active session.

use crate::session::types::ItemStatus;
use crate::{CliError, CliResult};

use super::{load_active, workspace_or_error};

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let Some(s) = load_active(&ws)? else {
        return Err(CliError::Other {
            message: "no active session".into(),
            exit_code: 1,
        });
    };

    let counts = s.bucket_counts();
    println!("id:      {}", s.id);
    println!("kind:    {}", s.kind);
    println!("subject: {}", s.subject);
    println!("started: {}", s.started_at);
    println!(
        "items:   {} open, {} accepted, {} rejected, {} pending",
        counts.open, counts.accepted, counts.rejected, counts.pending
    );
    if counts.open > 0 {
        println!();
        println!("open items:");
        for item in s.items.iter().filter(|i| i.status == ItemStatus::Open) {
            println!("  - {}", item.item_ref);
        }
    }
    Ok(())
}
