//! `aristo canon` subcommand family + shared canon-step helpers.
//!
//! `runner` is the API-call + cache-update primitive shared between
//! `aristo stamp` (PR #5) and `aristo critique` (PR #6). The
//! user-facing `aristo canon {list, refresh, unbind}` subcommands sit
//! alongside the trust-card renderer in `aristo show`.
//!
//! `accept` lands PR #7's atomic source rewrite + index rebind +
//! cache pending → accepted move for an accepted canon match.

pub(crate) mod accept;
pub(crate) mod catalogue;
pub(crate) mod list;
pub(crate) mod migrate;
pub(crate) mod probe;
pub(crate) mod refresh;
pub(crate) mod reject;
pub(crate) mod runner;
pub(crate) mod session_kind;
pub(crate) mod suggestions;
pub(crate) mod unbind;

use aristo_core::canon::{CanonClient, HttpCanonClient, MockCanonClient};

use crate::{CliError, CliResult};

/// The canon client for a command that cannot run without one
/// (`catalogue`, `migrate`): the test fixture wins, then the resolved
/// credential addressed at the org's repo for the checkout at `start`.
/// A resolver or target failure is the command's error, carrying the
/// diagnosis as is. `what` names the command in the message.
pub(crate) fn required_client(
    what: &str,
    start: &std::path::Path,
) -> CliResult<Box<dyn CanonClient>> {
    if let Some(mock) = MockCanonClient::from_env() {
        return Ok(Box::new(mock));
    }
    let fail = |e: aristo_core::auth::AuthError| CliError::Other {
        message: format!("{what} requires authentication: {e}"),
        exit_code: 1,
    };
    let creds = aristo_core::auth::resolve_full().map_err(fail)?;
    let t = crate::data_plane::resolve_target(&creds, start).map_err(fail)?;
    Ok(Box::new(HttpCanonClient::new(
        t.base_url,
        &creds.token,
        t.repo,
    )))
}
