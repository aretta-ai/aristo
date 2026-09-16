//! The org's repo directory: `GET <server>/_org/api/repos`.
//!
//! The conductor addresses every data-plane route by the org's repo
//! NAME (`/<name>/verify/sessions`, `/<name>/api/canon/match`), while
//! the CLI knows the checkout's GitHub `owner/repo`. This one read
//! bridges the two: the directory lists the repos the token admits,
//! each with its GitHub upstream, and [`repo_segment_for`] picks the
//! name for a checkout. Any grant of the org may read it.

use serde::Deserialize;

use super::error::AuthError;
use super::oauth::{build_agent, map_response, read_body_capped};
use super::token::Token;

/// One repo of the org as the directory lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrgRepo {
    /// The conductor's repo name — the path segment.
    pub name: String,
    /// The GitHub `owner/repo` this repo tracks (lowercase).
    pub github_repo: String,
}

/// Path of the directory route, relative to the data-plane base.
pub const ORG_REPOS_PATH: &str = "/_org/api/repos";

/// Fetch the repos the token admits at `base_url` (no trailing `/`).
pub fn fetch_org_repos(base_url: &str, token: &Token) -> Result<Vec<OrgRepo>, AuthError> {
    let agent = build_agent();
    let result = agent
        .get(format!("{base_url}{ORG_REPOS_PATH}"))
        .header("Authorization", &format!("Bearer {}", token.as_str()))
        .call();
    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = read_body_capped(resp, 1 << 20);
            map_response::<Vec<OrgRepo>>(status, &body)
        }
        Err(e) => Err(AuthError::Unreachable {
            server: base_url.to_string(),
            reason: e.to_string(),
        }),
    }
}

/// The path segment for the checkout `github_repo` (case-insensitive,
/// with or without a trailing `.git`), or `None` when the org does not
/// have that repo.
pub fn repo_segment_for<'a>(repos: &'a [OrgRepo], github_repo: &str) -> Option<&'a str> {
    let wanted = github_repo
        .trim()
        .trim_end_matches(".git")
        .to_ascii_lowercase();
    repos
        .iter()
        .find(|r| r.github_repo.to_ascii_lowercase() == wanted)
        .map(|r| r.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> Vec<OrgRepo> {
        vec![
            OrgRepo {
                name: "widgets".into(),
                github_repo: "acme/widgets".into(),
            },
            OrgRepo {
                name: "gadgets".into(),
                github_repo: "acme/gadgets".into(),
            },
        ]
    }

    #[test]
    fn segment_matches_case_insensitively_and_ignores_dot_git() {
        let d = dir();
        assert_eq!(repo_segment_for(&d, "acme/widgets"), Some("widgets"));
        assert_eq!(repo_segment_for(&d, "Acme/Widgets.git"), Some("widgets"));
        assert_eq!(repo_segment_for(&d, "acme/gadgets"), Some("gadgets"));
        assert_eq!(repo_segment_for(&d, "acme/nope"), None);
        assert_eq!(repo_segment_for(&[], "acme/widgets"), None);
    }

    #[test]
    fn directory_decodes_the_wire_shape() {
        let body = r#"[{"name":"widgets","github_repo":"acme/widgets"}]"#;
        let d: Vec<OrgRepo> = super::super::oauth::map_response(200, body).unwrap();
        assert_eq!(d, vec![dir()[0].clone()]);
    }
}
