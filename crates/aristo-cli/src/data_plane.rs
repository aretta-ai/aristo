//! Where a data-plane request goes: `<base>/<repo>/...`.
//!
//! The base is `ARETTA_API_URL` if set, else the credential's own
//! server ([`aristo_core::auth::data_plane_base`]). The `<repo>`
//! segment is the org's repo NAME, which the CLI learns from the org's
//! repo directory by matching the checkout's GitHub `owner/repo`
//! (0.7.1 git derivation, or `ARISTO_REPO`).

use std::path::Path;

use aristo_core::auth::store::{load_store, CredentialEntry, CredentialStore};
use aristo_core::auth::{
    data_plane_base, derive_repo_full_name, fetch_org_repos, repo_segment_for, AuthError, OrgRepo,
    ResolvedCreds, Token, SERVER_ENV_VAR,
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

/// Credentials and target together. The resolver's precedence stands;
/// when several servers are on file and none is named, the org whose
/// repo directory lists the checkout wins — exactly one match, else the
/// resolver's own "say which" error. No per-checkout pin is consulted.
pub(crate) fn resolve_creds_and_target(start: &Path) -> Result<(ResolvedCreds, Target), AuthError> {
    match aristo_core::auth::resolve_full() {
        Ok(creds) => {
            let t = resolve_target(&creds, start)?;
            Ok((creds, t))
        }
        Err(AuthError::SeveralServers { path, entries }) => {
            warn_if_instance_url_set();
            // No checkout to look up → nothing to decide by; the
            // resolver's error stands.
            let Ok(github_repo) = github_repo_for(start) else {
                return Err(AuthError::SeveralServers { path, entries });
            };
            let store = load_store()?;
            match select_by_directory(&store, &github_repo, fetch_org_repos) {
                Some((entry, target)) => Ok((
                    ResolvedCreds {
                        token: entry.token.clone(),
                        server: entry.server.clone(),
                        user_login: entry.user_login.clone(),
                        user_id: entry.user_id,
                    },
                    target,
                )),
                None => Err(AuthError::SeveralServers { path, entries }),
            }
        }
        Err(e) => Err(e),
    }
}

/// The one stored entry whose org lists `github_repo`, with the target
/// it resolves to. `None` when no entry does, or more than one does; a
/// directory that cannot be read counts as not listing it. `lookup` is
/// the org directory fetch, injectable for tests.
fn select_by_directory<'a>(
    store: &'a CredentialStore,
    github_repo: &str,
    lookup: impl Fn(&str, &Token) -> Result<Vec<OrgRepo>, AuthError>,
) -> Option<(&'a CredentialEntry, Target)> {
    let mut hits = store.entries.iter().filter_map(|e| {
        let base_url = data_plane_base(None, &e.server);
        let repos = lookup(&base_url, &e.token).ok()?;
        let repo = repo_segment_for(&repos, github_repo)?.to_string();
        Some((e, Target { base_url, repo }))
    });
    let first = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(first)
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

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::auth::ServerUrl;

    fn store(servers: &[&str]) -> CredentialStore {
        CredentialStore {
            entries: servers
                .iter()
                .map(|s| {
                    CredentialEntry::bare(Token::new(format!("arta_{s}")), ServerUrl::parse(s))
                })
                .collect(),
        }
    }

    fn repo(name: &str, github_repo: &str) -> OrgRepo {
        OrgRepo {
            name: name.into(),
            github_repo: github_repo.into(),
        }
    }

    /// Directory per server: a.example has widgets, b.example has gadgets.
    fn directory(base: &str, _token: &Token) -> Result<Vec<OrgRepo>, AuthError> {
        match base {
            "https://a.example" => Ok(vec![repo("widgets", "acme/widgets")]),
            "https://b.example" => Ok(vec![repo("gadgets", "acme/gadgets")]),
            other => Err(AuthError::Unreachable {
                server: other.into(),
                reason: "no such org".into(),
            }),
        }
    }

    #[test]
    fn the_one_org_listing_the_checkout_wins() {
        let s = store(&["https://a.example", "https://b.example"]);
        let (entry, target) = select_by_directory(&s, "acme/widgets", directory).expect("a match");
        assert_eq!(entry.server.as_str(), "https://a.example");
        assert_eq!(target.base_url, "https://a.example");
        assert_eq!(target.repo, "widgets");

        let (entry, target) =
            select_by_directory(&s, "Acme/Gadgets.git", directory).expect("a match");
        assert_eq!(entry.server.as_str(), "https://b.example");
        assert_eq!(target.repo, "gadgets");
    }

    #[test]
    fn no_org_listing_it_is_none() {
        let s = store(&["https://a.example", "https://b.example"]);
        assert!(select_by_directory(&s, "acme/nope", directory).is_none());
    }

    #[test]
    fn several_orgs_listing_it_is_none() {
        let s = store(&["https://a.example", "https://b.example"]);
        let both = |_: &str, _: &Token| Ok(vec![repo("widgets", "acme/widgets")]);
        assert!(select_by_directory(&s, "acme/widgets", both).is_none());
    }

    #[test]
    fn an_unreachable_directory_counts_as_not_listing() {
        let s = store(&["https://a.example", "https://down.example"]);
        let (entry, _) = select_by_directory(&s, "acme/widgets", directory).expect("a match");
        assert_eq!(entry.server.as_str(), "https://a.example");
    }
}
