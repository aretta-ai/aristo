//! Where a data-plane request goes: `<base>/<repo>/...`.
//!
//! The base is `ARETTA_API_URL` if set, else the credential's own
//! server ([`aristo_core::auth::data_plane_base`]). The `<repo>`
//! segment is the org's repo NAME, which the CLI learns from the org's
//! repo directory by matching the checkout's GitHub `owner/repo`
//! (0.7.1 git derivation, or `ARISTO_REPO`).

use std::path::Path;

use aristo_core::auth::{
    data_plane_base, derive_repo_full_name, fetch_org_repos, repo_segment_for, AuthError,
    ResolvedCreds, SERVER_ENV_VAR,
};

use crate::workspace::Workspace;

/// Everything a verify or canon client needs to address the server.
pub(crate) struct Target {
    /// `ARETTA_API_URL`, else the credential's server. No trailing `/`.
    pub base_url: String,
    /// The org's repo name — the path segment.
    pub repo: String,
}

/// The checkout's GitHub `owner/repo`: `ARISTO_REPO` when set (the CI
/// escape hatch), else derived from git at or above `start`. No network.
pub(crate) fn github_repo_for(start: &Path) -> Result<String, AuthError> {
    if let Some(r) = std::env::var("ARISTO_REPO")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        return Ok(r.trim().to_string());
    }
    derive_repo_full_name(start)
}

/// Resolve the target for `creds` and the checkout at or above `start`.
/// One round trip to the org's repo directory; a repo the org does not
/// have is [`AuthError::RepoNotInOrg`], an unreachable server is
/// [`AuthError::Unreachable`].
pub(crate) fn resolve_target(creds: &ResolvedCreds, start: &Path) -> Result<Target, AuthError> {
    warn_if_instance_url_set();
    let base_url = data_plane_base(std::env::var(SERVER_ENV_VAR).ok().as_deref(), &creds.server);
    let github_repo = github_repo_for(start)?;
    let repos =
        fetch_org_repos(&base_url, &creds.token).map_err(|e| e.for_checkout(&github_repo))?;
    let repo = repo_segment_for(&repos, &github_repo)
        .ok_or_else(|| AuthError::RepoNotInOrg {
            github_repo: github_repo.clone(),
            server: base_url.clone(),
        })?
        .to_string();
    Ok(Target { base_url, repo })
}

/// The directory a command should derive its checkout from: the
/// workspace root when there is one, else the current directory.
pub(crate) fn checkout_start() -> std::path::PathBuf {
    Workspace::find(None)
        .map(|ws| ws.root)
        .or_else(|_| std::env::current_dir())
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
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
