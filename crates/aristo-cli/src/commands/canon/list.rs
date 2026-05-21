//! `aristo canon list` — read-only summary of `.aristo/canon-matches.toml`.
//!
//! Renders one line per annotation with its match buckets, so the
//! user can audit their canon-binding state at a glance. The render
//! is human-readable; future extensions can add `--json` if needed
//! (parallel to `aristo list`).

use std::fs;

use aristo_core::canon::cache::CanonMatchesFile;

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult};

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let cache_path = ws.canon_matches_path();
    let cache = if cache_path.is_file() {
        let raw = fs::read_to_string(&cache_path).map_err(CliError::Io)?;
        toml::from_str::<CanonMatchesFile>(&raw).map_err(|e| CliError::Other {
            message: format!("parsing {}: {e}", cache_path.display()),
            exit_code: 1,
        })?
    } else {
        println!("ok: no canon matches yet. Run `aristo stamp` to populate the cache.");
        return Ok(());
    };

    if cache.entries.is_empty() {
        println!("ok: no canon matches in .aristo/canon-matches.toml.");
        return Ok(());
    }

    let mut pending_total = 0usize;
    let mut accepted_total = 0usize;
    let mut rejected_total = 0usize;

    println!("canon matches:");
    for (id, entry) in &cache.entries {
        let p = entry.pending_matches.len();
        let a = entry.accepted_matches.len();
        let r = entry.rejected_matches.len();
        pending_total += p;
        accepted_total += a;
        rejected_total += r;
        println!(
            "  {id:<60}  pending={p}  accepted={a}  rejected={r}",
            id = id.as_str()
        );
        for m in &entry.pending_matches {
            println!(
                "    [pending]  {} {} (conf {:.2}, {} tier)",
                m.canon_id,
                m.version,
                m.confidence,
                m.prefix_tier.as_prefix(),
            );
        }
        for m in &entry.accepted_matches {
            println!(
                "    [accepted] {} {} (conf {:.2}, {} tier, bound_at {})",
                m.canon_id,
                m.version,
                m.confidence,
                m.prefix_tier.as_prefix(),
                m.bound_at,
            );
        }
        for m in &entry.rejected_matches {
            let reason = m.reason.as_deref().unwrap_or("—");
            println!(
                "    [rejected] {} {} (reason: {reason})",
                m.canon_id, m.version
            );
        }
    }
    println!();
    println!(
        "totals: {pending_total} pending, {accepted_total} accepted, {rejected_total} rejected ({} annotation(s))",
        cache.entries.len()
    );
    Ok(())
}
