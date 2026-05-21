//! Auto-derive `repo_full_name` (`owner/repo`) from a workspace's
//! `.git/config`.
//!
//! Parses the INI-shaped git config to find the `remote.origin.url`
//! field, then extracts the `owner/repo` slug from either form:
//!
//! - `https://github.com/owner/repo(.git)?`
//! - `git@github.com:owner/repo(.git)?`
//! - `ssh://git@github.com/owner/repo(.git)?`
//!
//! The `aristo auth login` CLI calls this with the workspace root as
//! its starting point; if it can't resolve a repo, it surfaces a
//! clear "pass `--repo <owner/repo>` explicitly" diagnostic.

use std::fs;
use std::path::Path;

use super::error::AuthError;

/// Derive `owner/repo` from `<workspace_root>/.git/config`'s
/// `[remote "origin"]` URL.
///
/// Returns `AuthError::Malformed` with a clear message when:
/// - the workspace has no `.git/` directory,
/// - the config file has no `[remote "origin"]` section,
/// - the section has no `url =` field,
/// - the URL doesn't match a known GitHub form.
pub fn derive_repo_full_name(workspace_root: &Path) -> Result<String, AuthError> {
    let config_path = workspace_root.join(".git").join("config");
    if !config_path.is_file() {
        return Err(AuthError::Malformed(format!(
            "no .git/config at {} — pass `--repo <owner/repo>` to scope the token explicitly",
            config_path.display()
        )));
    }
    let raw = fs::read_to_string(&config_path)
        .map_err(|e| AuthError::Malformed(format!("read {}: {e}", config_path.display())))?;
    let url = extract_origin_url(&raw).ok_or_else(|| {
        AuthError::Malformed(format!(
            "no `[remote \"origin\"]` url in {} — pass `--repo <owner/repo>`",
            config_path.display()
        ))
    })?;
    parse_github_url(&url).ok_or_else(|| {
        AuthError::Malformed(format!(
            "remote.origin.url `{url}` doesn't look like a GitHub URL — pass `--repo <owner/repo>`"
        ))
    })
}

/// Pure: scan the INI-shaped git config text for the value of the
/// `url =` line inside the `[remote "origin"]` section.
fn extract_origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for raw_line in config.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            // Section header. Match `[remote "origin"]` permissively
            // (accept extra whitespace).
            let inside = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_origin = inside.eq_ignore_ascii_case("remote \"origin\"");
            continue;
        }
        if in_origin {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim().eq_ignore_ascii_case("url") {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}

/// Pure: extract `owner/repo` from a GitHub URL of any of the three
/// supported forms. Returns `None` if the URL doesn't look like
/// GitHub.
fn parse_github_url(url: &str) -> Option<String> {
    // Form 1: https://github.com/owner/repo(.git)?
    if let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        return parse_owner_slash_repo(rest);
    }
    // Form 2: git@github.com:owner/repo(.git)?
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return parse_owner_slash_repo(rest);
    }
    // Form 3: ssh://git@github.com/owner/repo(.git)?
    if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        return parse_owner_slash_repo(rest);
    }
    None
}

fn parse_owner_slash_repo(rest: &str) -> Option<String> {
    let cleaned = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = cleaned.splitn(3, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    // owner/repo must not contain control characters or spaces.
    if owner.chars().any(|c| c.is_whitespace()) || repo.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn extract_origin_url_finds_url_in_origin_section() {
        let config = r#"
[core]
    repositoryformatversion = 0

[remote "origin"]
    url = https://github.com/foo/bar.git
    fetch = +refs/heads/*:refs/remotes/origin/*

[branch "main"]
    remote = origin
"#;
        assert_eq!(
            extract_origin_url(config).as_deref(),
            Some("https://github.com/foo/bar.git")
        );
    }

    #[test]
    fn extract_origin_url_ignores_other_remote_sections() {
        let config = r#"
[remote "upstream"]
    url = https://github.com/upstream/fork.git

[remote "origin"]
    url = https://github.com/me/repo.git
"#;
        assert_eq!(
            extract_origin_url(config).as_deref(),
            Some("https://github.com/me/repo.git")
        );
    }

    #[test]
    fn extract_origin_url_handles_comments_and_blank_lines() {
        let config = "# leading comment\n\n[remote \"origin\"]\n; mid comment\n    url = git@github.com:o/r.git\n";
        assert_eq!(
            extract_origin_url(config).as_deref(),
            Some("git@github.com:o/r.git")
        );
    }

    #[test]
    fn extract_origin_url_returns_none_when_no_origin_section() {
        let config = "[core]\n    repositoryformatversion = 0\n";
        assert_eq!(extract_origin_url(config), None);
    }

    #[test]
    fn parse_github_url_https_form_strips_git_suffix() {
        assert_eq!(
            parse_github_url("https://github.com/foo/bar.git").as_deref(),
            Some("foo/bar")
        );
        assert_eq!(
            parse_github_url("https://github.com/foo/bar").as_deref(),
            Some("foo/bar")
        );
    }

    #[test]
    fn parse_github_url_ssh_short_form() {
        assert_eq!(
            parse_github_url("git@github.com:foo/bar.git").as_deref(),
            Some("foo/bar")
        );
        assert_eq!(
            parse_github_url("git@github.com:foo/bar").as_deref(),
            Some("foo/bar")
        );
    }

    #[test]
    fn parse_github_url_ssh_url_form() {
        assert_eq!(
            parse_github_url("ssh://git@github.com/foo/bar.git").as_deref(),
            Some("foo/bar")
        );
    }

    #[test]
    fn parse_github_url_non_github_returns_none() {
        assert_eq!(parse_github_url("https://gitlab.com/foo/bar.git"), None);
        assert_eq!(parse_github_url("https://example.com/foo/bar.git"), None);
        assert_eq!(parse_github_url("https://bitbucket.org/foo/bar"), None);
    }

    #[test]
    fn parse_github_url_rejects_short_paths() {
        assert_eq!(parse_github_url("https://github.com/foo"), None);
        assert_eq!(parse_github_url("git@github.com:foo"), None);
    }

    #[test]
    fn derive_repo_full_name_reads_workspace_dot_git_config() {
        let tmp = TempDir::new().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            git_dir.join("config"),
            r#"
[core]
    repositoryformatversion = 0

[remote "origin"]
    url = https://github.com/owner/repo.git
"#,
        )
        .unwrap();
        let r = derive_repo_full_name(tmp.path()).expect("derive ok");
        assert_eq!(r, "owner/repo");
    }

    #[test]
    fn derive_repo_full_name_without_dot_git_yields_helpful_error() {
        let tmp = TempDir::new().unwrap();
        // No .git directory.
        let err = derive_repo_full_name(tmp.path()).expect_err("should fail");
        match err {
            AuthError::Malformed(m) => {
                assert!(m.contains("no .git/config"), "got: {m}");
                assert!(m.contains("--repo"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn derive_repo_full_name_with_non_github_remote_yields_helpful_error() {
        let tmp = TempDir::new().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            git_dir.join("config"),
            r#"
[remote "origin"]
    url = https://gitlab.com/foo/bar.git
"#,
        )
        .unwrap();
        let err = derive_repo_full_name(tmp.path()).expect_err("should fail");
        match err {
            AuthError::Malformed(m) => {
                assert!(m.contains("doesn't look like a GitHub URL"), "got: {m}");
                assert!(m.contains("--repo"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }
}
