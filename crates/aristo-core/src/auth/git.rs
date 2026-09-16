//! Auto-derive `repo_full_name` (`owner/repo`) from the checkout's git
//! config, found the way git finds it: walk up from the starting
//! directory to the nearest `.git`; a directory holds `config` itself,
//! a file (linked worktree, submodule) names a `gitdir` whose own
//! `config` or `commondir` leads to it. So a subdirectory, a worktree
//! and a plain clone all derive the same repo `git config` reports.
//!
//! Parses the INI-shaped git config to find the `remote.origin.url`
//! field, then extracts the `owner/repo` slug from either form:
//!
//! - `https://github.com/owner/repo(.git)?`
//! - `git@github.com:owner/repo(.git)?`
//! - `ssh://git@github.com/owner/repo(.git)?`
//!
//! Every place that needs the checkout's repo — the credential
//! resolver, the login default, `auth token` / `auth logout`, verify
//! dispatch — calls this; if it can't resolve a repo, it surfaces a
//! clear "set ARISTO_REPO=<owner/repo>" diagnostic.

use std::fs;
use std::path::{Path, PathBuf};

use super::error::AuthError;

/// Derive `owner/repo` from the `[remote "origin"]` URL of the git
/// config that governs `start` (see the module doc for how it is found).
///
/// Returns `AuthError::Malformed` with a clear message when:
/// - no git repository is found at or above `start`,
/// - the config file has no `[remote "origin"]` section,
/// - the section has no `url =` field,
/// - the URL doesn't match a known GitHub form.
pub fn derive_repo_full_name(start: &Path) -> Result<String, AuthError> {
    let config_path = git_config_path(start).ok_or_else(|| {
        AuthError::Malformed(format!(
            "no git repository at or above {} — run from a checkout, or set ARISTO_REPO=<owner/repo>",
            start.display()
        ))
    })?;
    let raw = fs::read_to_string(&config_path)
        .map_err(|e| AuthError::Malformed(format!("read {}: {e}", config_path.display())))?;
    let url = extract_origin_url(&raw).ok_or_else(|| {
        AuthError::Malformed(format!(
            "no `[remote \"origin\"]` url in {} — set ARISTO_REPO=<owner/repo>",
            config_path.display()
        ))
    })?;
    parse_github_url(&url).ok_or_else(|| {
        AuthError::Malformed(format!(
            "remote.origin.url `{url}` doesn't look like a GitHub URL — set ARISTO_REPO=<owner/repo>"
        ))
    })
}

/// The `config` file governing `start`: walk up to the nearest `.git`.
/// A directory is the repository itself. A file names a `gitdir:`
/// (absolute, or relative to the file's directory); that gitdir holds
/// its own `config` (a submodule) or a `commondir` pointing at the
/// common repository that does (a linked worktree). `None` when no
/// `.git` exists at or above `start`, or the pointer chain is broken.
fn git_config_path(start: &Path) -> Option<PathBuf> {
    let dot_git = start
        .ancestors()
        .map(|dir| dir.join(".git"))
        .find(|p| p.exists())?;
    let gitdir = if dot_git.is_dir() {
        dot_git
    } else {
        let pointer = fs::read_to_string(&dot_git).ok()?;
        let target = pointer.strip_prefix("gitdir:")?.trim();
        let base = dot_git.parent()?;
        base.join(target)
    };
    let own = gitdir.join("config");
    if own.is_file() {
        return Some(own);
    }
    let common = fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common_config = gitdir.join(common.trim()).join("config");
    common_config.is_file().then_some(common_config)
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

    fn write_config(dir: &std::path::Path, url: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("config"),
            format!("[remote \"origin\"]\n    url = {url}\n"),
        )
        .unwrap();
    }

    #[test]
    fn derive_walks_up_from_a_subdirectory_to_the_repo_root() {
        let tmp = TempDir::new().unwrap();
        write_config(
            &tmp.path().join(".git"),
            "https://github.com/owner/repo.git",
        );
        let deep = tmp.path().join("crates/core/src");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(derive_repo_full_name(&deep).unwrap(), "owner/repo");
    }

    #[test]
    fn derive_follows_a_worktree_dot_git_file_to_the_common_config() {
        // A linked worktree: `.git` is a FILE naming its gitdir under the
        // main repo's `.git/worktrees/<name>`, whose `commondir` points
        // back at the common `.git` that holds `config`.
        let tmp = TempDir::new().unwrap();
        let main_git = tmp.path().join("main/.git");
        write_config(&main_git, "git@github.com:owner/repo.git");
        let wt_gitdir = main_git.join("worktrees/wt-linked");
        std::fs::create_dir_all(&wt_gitdir).unwrap();
        std::fs::write(wt_gitdir.join("commondir"), "../..\n").unwrap();
        let wt = tmp.path().join("wt-linked");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", wt_gitdir.display()),
        )
        .unwrap();
        assert_eq!(derive_repo_full_name(&wt).unwrap(), "owner/repo");
        // From a subdirectory of the worktree too.
        assert_eq!(
            derive_repo_full_name(&wt.join("src")).unwrap(),
            "owner/repo"
        );
    }

    #[test]
    fn derive_follows_a_dot_git_file_whose_gitdir_has_its_own_config() {
        // A submodule checkout: `.git` is a file, and the gitdir it names
        // carries its own `config` (no `commondir`).
        let tmp = TempDir::new().unwrap();
        let modules_git = tmp.path().join("super/.git/modules/sub");
        write_config(&modules_git, "https://github.com/owner/sub.git");
        let sub = tmp.path().join("super/sub");
        std::fs::create_dir_all(&sub).unwrap();
        // Relative gitdir, resolved against the directory holding `.git`.
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(derive_repo_full_name(&sub).unwrap(), "owner/sub");
    }

    #[test]
    fn derive_repo_full_name_without_dot_git_yields_helpful_error() {
        let tmp = TempDir::new().unwrap();
        // No .git directory.
        let err = derive_repo_full_name(tmp.path()).expect_err("should fail");
        match err {
            AuthError::Malformed(m) => {
                assert!(m.contains("no git repository"), "got: {m}");
                assert!(m.contains("ARISTO_REPO"), "got: {m}");
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
                assert!(m.contains("ARISTO_REPO"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }
}
