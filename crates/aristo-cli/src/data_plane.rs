//! Resolve the base URL for **data-plane** requests — verify session
//! dispatch and canon match: `ARETTA_API_URL` (env) if set, else the
//! credential's own server. The precedence itself lives in
//! [`aristo_core::auth::data_plane_base`]; this wrapper only gathers
//! the impure inputs and delegates.

use aristo_core::auth::{data_plane_base, ServerUrl, SERVER_ENV_VAR};

use crate::workspace::Workspace;

/// Base URL for verify/canon data-plane requests, given the signed-in
/// credential's `server`. Reads `ARETTA_API_URL` and, best-effort, the
/// nearest `aristo.toml` — only to warn about an ignored
/// `[instance] url`.
pub(crate) fn resolve_base(server: &ServerUrl) -> String {
    warn_if_instance_url_set();
    let env_override = std::env::var(SERVER_ENV_VAR).ok();
    data_plane_base(env_override.as_deref(), server)
}

/// `[instance] url` does not steer the data plane; a repo that carries
/// it would otherwise silently get a different server than the one its
/// owner believes is pinned.
fn warn_if_instance_url_set() {
    let Some(url) = Workspace::find(None)
        .ok()
        .and_then(|ws| ws.load_config().instance.url)
    else {
        return;
    };
    eprintln!(
        "warning: aristo.toml `[instance] url = \"{url}\"` is ignored — the data plane is \
         the credential's server, and {SERVER_ENV_VAR} overrides it. Remove the section."
    );
}
