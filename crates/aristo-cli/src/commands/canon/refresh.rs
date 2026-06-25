//! `aristo canon refresh` — invalidate the canon-match cache and
//! re-run the match call against every annotation in the index.
//!
//! Equivalent to `aristo stamp --refresh-canon`, but doesn't re-run
//! the stamp pipeline itself (no source walk, no body-hash drift
//! check, no index rewrite). Useful when:
//!
//! - The user knows the catalog has a new version they want to pull
//!   matches against, but doesn't need a fresh stamp.
//! - A `Degraded` outcome in a prior stamp left the cache partial;
//!   refresh re-tries the API call without other work.
//!
//! Same client-selection precedence as the runner; same graceful-
//! degradation handling for unreachable / unauth'd / 5xx responses.

use aristo_core::canon::cache::CanonMatchesFile;

use crate::commands::canon::runner::{print_stamp_summary, run_canon_step, RunnerArgs};
use crate::commands::index::workspace_or_error;
use crate::CliResult;

pub(crate) fn run() -> CliResult<()> {
    let ws = workspace_or_error()?;
    let config = ws.load_config();
    let index = crate::commands::show::load_index(&ws)?;

    let outcome = run_canon_step(RunnerArgs {
        ws: &ws,
        config: &config.canon,
        index: &index,
        skip_flag: false,
        refresh_flag: true,
        threshold: config.canon.threshold_stamp,
        found_by: "aristo canon refresh",
    })?;
    let cache_path = ws.canon_matches_path();
    let cache = CanonMatchesFile::read(&cache_path).unwrap_or_default();
    print_stamp_summary(&outcome, &cache, &ws);
    Ok(())
}
