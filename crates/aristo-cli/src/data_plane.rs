//! Resolve the base URL for **data-plane** requests — verify session
//! dispatch and canon match — from `ARETTA_API_URL` (env) > the
//! project's `aristo.toml` `[instance] url` > the signed-in account
//! server. The precedence itself lives in
//! [`aristo_core::auth::data_plane_base`]; this wrapper only gathers
//! the impure inputs (the real env var + the nearest workspace's
//! config) and delegates.

use aristo_core::auth::{data_plane_base, ServerUrl};

use crate::workspace::Workspace;

/// Base URL for verify/canon data-plane requests, given the signed-in
/// account `server` (from resolved credentials).
///
/// Reads `ARETTA_API_URL` and the nearest `aristo.toml`'s
/// `[instance] url` best-effort: a command run outside a workspace (or
/// with a malformed config) still resolves via env + `server`, so this
/// never fails. `ARETTA_API_URL` and `[instance] url` take precedence
/// over `server`, so a repo (or CI) can target a per-repo conductor
/// without re-authenticating.
pub(crate) fn resolve_base(server: &ServerUrl) -> String {
    let env_override = std::env::var("ARETTA_API_URL").ok();
    let instance = Workspace::find(None)
        .ok()
        .and_then(|ws| ws.load_config().instance.url);
    data_plane_base(env_override.as_deref(), instance.as_deref(), server)
}
