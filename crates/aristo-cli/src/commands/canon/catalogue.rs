//! `aristo canon catalogue` — download the canon catalogue (the full
//! list of available canon entries) from the data plane to a local,
//! gitignored snapshot at `.aristo/catalogue.json`, then print a
//! summary.
//!
//! Logged-in only: it presents the account's Bearer token to the
//! per-repo conductor's `GET /catalogue` (Read-gated), addressed via
//! the resolved data-plane base (`[instance] url` when set). The
//! download is a regenerable local cache — `aristo init` gitignores it
//! (see `ARISTO_GITIGNORE_ENTRIES`) — so agents can browse/search the
//! corpus offline without re-fetching, and it is never committed.

use std::collections::BTreeMap;

use aristo_core::canon::{
    CanonCatalogue, CanonClient, CanonError, HttpCanonClient, MockCanonClient,
};

use crate::commands::index::workspace_or_error;
use crate::{CliError, CliResult};

/// Workspace-relative path of the downloaded snapshot. Kept in sync
/// with the `.aristo/catalogue.json` entry in `init`'s
/// `ARISTO_GITIGNORE_ENTRIES`.
const CATALOGUE_REL: &str = ".aristo/catalogue.json";

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;

    // Client selection mirrors the canon runner/migrate: fixture (tests)
    // wins, then authenticated HTTP, else a logged-out error.
    let client: Box<dyn CanonClient> = if let Some(mock) = MockCanonClient::from_env() {
        Box::new(mock)
    } else {
        match aristo_core::auth::resolve_full() {
            Ok(creds) => {
                let base_url = crate::data_plane::resolve_base(&creds.server);
                Box::new(HttpCanonClient::new(base_url, &creds.token))
            }
            Err(_) => {
                return Err(CliError::Other {
                    message: "canon catalogue requires authentication.\n  \
                              Run `aristo auth login` first."
                        .into(),
                    exit_code: 1,
                });
            }
        }
    };

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
    if total == 0 {
        println!(
            "   note: the catalogue is empty — the instance has no canon corpus configured, \
             or `[instance] url` isn't set to a conductor."
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
