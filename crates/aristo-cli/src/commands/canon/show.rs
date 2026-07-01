//! `aristo canon show <canon_id>` — render the full per-entry canon
//! detail view.
//!
//! Coming soon. The canon detail view is being reworked to resolve
//! against the per-instance data plane (see `[instance]` in
//! `aristo.toml`); until that lands the command reports the deferral
//! instead of calling a canon endpoint. Prior behavior: `GET
//! /canon/entry/<canon_id>` rendered category / property / backing /
//! statement / references for the entry.

use crate::{CliError, CliResult};

pub(crate) fn run(_canon_id: &str, _version: Option<String>) -> CliResult<()> {
    Err(CliError::NotImplemented {
        what: "aristo canon show",
        hint: "coming soon (reworking for per-instance data-plane routing)",
    })
}
