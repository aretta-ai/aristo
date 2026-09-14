//! `aristo canon` subcommand family + shared canon-step helpers.
//!
//! `runner` is the API-call + cache-update primitive shared between
//! `aristo stamp` (PR #5) and `aristo critique` (PR #6). The
//! user-facing `aristo canon {show, list, refresh, unbind,
//! request-verify}` subcommands land in PR #8/#9 alongside the
//! trust-card renderer.
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
pub(crate) mod request_verify;
pub(crate) mod runner;
pub(crate) mod session_kind;
pub(crate) mod show;
pub(crate) mod suggestions;
pub(crate) mod unbind;

use aristo_core::canon::{CanonClient, HttpCanonClient, MockCanonClient};

use crate::{CliError, CliResult};

/// The canon client for a command that cannot run without one
/// (`catalogue`, `migrate`): the test fixture wins, then the resolved
/// credential. A resolver failure is the command's error, carrying the
/// resolver's own diagnosis — "nothing on file" says to log in;
/// "credentials on file, none for this checkout" names the checkout,
/// the entries and the fixes. `what` names the command in the message.
pub(crate) fn required_client(what: &str) -> CliResult<Box<dyn CanonClient>> {
    if let Some(mock) = MockCanonClient::from_env() {
        return Ok(Box::new(mock));
    }
    match aristo_core::auth::resolve_full() {
        Ok(creds) => {
            let base_url = crate::data_plane::resolve_base(&creds.server);
            Ok(Box::new(HttpCanonClient::new(base_url, &creds.token)))
        }
        Err(e) => Err(CliError::Other {
            message: format!("{what} requires authentication: {e}"),
            exit_code: 1,
        }),
    }
}
