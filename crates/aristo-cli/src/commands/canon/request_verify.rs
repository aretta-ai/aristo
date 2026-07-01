//! `aristo canon request-verify <canon_id> [--notes <text>]` — record
//! a demand signal for verification backing on a canon entry.
//!
//! Coming soon. Demand-signal submission is being reworked to resolve
//! against the per-instance data plane (see `[instance]` in
//! `aristo.toml`); until that lands the command reports the deferral
//! instead of calling a canon endpoint. Prior behavior: `POST
//! /canon/request-verify` recorded an idempotent demand signal.

use crate::{CliError, CliResult};

pub(crate) fn run(_canon_id: &str, _notes: Option<String>) -> CliResult<()> {
    Err(CliError::NotImplemented {
        what: "aristo canon request-verify",
        hint: "coming soon (reworking for per-instance data-plane routing)",
    })
}
