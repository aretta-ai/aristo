//! `aristo canon catalogue` — download the canon catalogue (the full
//! list of available canon entries) from the data plane to a local,
//! gitignored snapshot at `.aristo/catalogue.json`, then print a
//! summary.
//!
//! Logged-in only: it presents the account's Bearer token to the
//! org server's `GET /catalogue` (Read-gated), addressed via
//! the resolved data-plane base. The
//! download is a regenerable local cache — `aristo init` gitignores it
//! (see `ARISTO_GITIGNORE_ENTRIES`) — so agents can browse/search the
//! corpus offline without re-fetching, and it is never committed.

use std::collections::BTreeMap;

use aristo_core::canon::{CanonCatalogue, CanonError};

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult};

/// Workspace-relative path of the downloaded snapshot. Kept in sync
/// with the `.aristo/catalogue.json` entry in `init`'s
/// `ARISTO_GITIGNORE_ENTRIES`.
const CATALOGUE_REL: &str = ".aristo/catalogue.json";

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;

    // Fixture (tests) wins, then the resolved credential; a resolver
    // failure is the command's error with the resolver's diagnosis.
    let client = super::required_client("canon catalogue", &ws.root)?;

    let catalogue = client.catalogue().map_err(canon_error_to_cli)?;

    // Persist the snapshot (pretty JSON) to the gitignored local cache.
    let aristo_dir = ws.aristo_dir();
    std::fs::create_dir_all(&aristo_dir).map_err(CliError::Io)?;
    let out_path = aristo_dir.join("catalogue.json");
    let json = serde_json::to_string_pretty(&catalogue).map_err(|e| CliError::Other {
        message: format!("serializing catalogue: {e}"),
        exit_code: 1,
    })?;
    std::fs::write(&out_path, json).map_err(CliError::Io)?;

    print_summary(&catalogue);
    Ok(())
}

fn print_summary(catalogue: &CanonCatalogue) {
    let total = catalogue.entries.len();
    println!(
        "ok: downloaded {total} canon entr{} to {CATALOGUE_REL} (gitignored local snapshot)",
        if total == 1 { "y" } else { "ies" }
    );
    match &catalogue.serving {
        Some(s) if s.is_empty_edition() => {
            println!(
                "   note: no served edition for this repository — the org has no book for it yet, \
                 so there are no canon entries to match. Nothing to do here until an edition is served."
            );
            return;
        }
        Some(s) if s.state == "unavailable" => {
            println!(
                "   note: the served edition is unavailable (reason: {}) — the list may be empty or stale.",
                s.reason.as_deref().unwrap_or("unknown")
            );
            if total == 0 {
                return;
            }
        }
        Some(s) => {
            if let Some(edition) = &s.edition {
                println!("   served edition: {edition}");
            }
        }
        None => {}
    }
    if total == 0 {
        println!(
            "   note: the catalogue is empty — this server has no canon corpus configured \
             (check `aristo auth status` for which server this checkout resolves to)."
        );
        return;
    }
    let backed = catalogue
        .entries
        .iter()
        .filter(|e| e.tier_label() == "aristos")
        .count();
    println!(
        "   backed (aristos): {backed} · unbacked (kanon): {}",
        total - backed
    );
    // Per-category counts, sorted, so the readout is browsable at a glance.
    let mut by_category: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &catalogue.entries {
        *by_category.entry(e.category.as_str()).or_default() += 1;
    }
    println!("   by category:");
    for (cat, n) in &by_category {
        println!("     {cat}: {n}");
    }
    println!("   read/search the full snapshot at {CATALOGUE_REL}");
}

fn canon_error_to_cli(e: CanonError) -> CliError {
    CliError::Other {
        message: format!("canon catalogue error: {e}"),
        exit_code: 1,
    }
}
