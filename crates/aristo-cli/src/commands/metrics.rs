//! `aristo metrics` — the machine-readable project-metrics surface
//! (Phase 18 #9). Emits the index-derived [`Metrics`] as JSON (`--json`) or a
//! short human summary.
//!
//! The nudge/progress engine computes the same [`Metrics`] value internally;
//! this command exposes it for tooling and the `aristo-status` skill (#12).
//! It is read-only against the index (it walks source only for the coverage
//! denominator the tier formula needs), so it never mutates the workspace.

use aristo_core::metrics::Metrics;
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

use crate::commands::index::workspace_or_error;
use crate::commands::show::load_index;
use crate::{CliError, CliResult};

pub(crate) fn run(json: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let index = load_index(&ws)?;

    // Walk source for the per-module fn surface the coverage score needs as
    // its denominator — same conservative `WalkOptions::none()` the badge
    // command uses (read-only, no config write path).
    let fn_counts =
        count_fns_per_module_with(&ws.root, &WalkOptions::none()).map_err(|e| CliError::Other {
            message: format!("failed to walk source for metrics coverage: {e}"),
            exit_code: 1,
        })?;
    let default_method = ws.load_config().verify.default_method;
    let metrics = Metrics::from_index(&index, &fn_counts, default_method);

    if json {
        let out = serde_json::to_string_pretty(&metrics).map_err(|e| CliError::Other {
            message: format!("serializing metrics: {e}"),
            exit_code: 1,
        })?;
        println!("{out}");
    } else {
        print_human(&metrics);
    }
    Ok(())
}

fn print_human(m: &Metrics) {
    let pct = (m.verification_rate * 100.0).round() as u32;
    println!("Aristo metrics");
    println!("  intents:     {}", m.intents);
    println!("  assumes:     {}", m.assumes);
    println!("  verifiable:  {}", m.verifiable);
    println!(
        "  verified:    {} / {} ({pct}%)",
        m.verified_clean, m.verifiable
    );
    println!("  unverified:  {}", m.unverified);
    println!(
        "  tier:        {} (score {:.2})",
        m.tier.label(),
        m.visible_score
    );
}
