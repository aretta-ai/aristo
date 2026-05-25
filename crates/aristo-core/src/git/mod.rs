//! Thin git shell-out helpers.
//!
//! Aristo deliberately avoids pulling in `git2` / `gix` — those are
//! heavy dependencies for a handful of operations. We invoke the
//! user's `git` binary on the system PATH and parse its stdout. The
//! invocations here are intentionally narrow:
//!
//! - [`rev_parse_head`] resolves `HEAD` to a 40-char SHA.
//! - [`commit_present_on_remote`] checks whether a commit SHA appears
//!   on any `origin/*` ref via `git branch -r --contains <sha>`. Used
//!   by the canon-verify push-first precheck (per WORKFLOW.md §4 +
//!   §7c "Push-first; mutagen later").
//!
//! Tests use real `git init` against a tempdir — the small extra
//! cost (≈ tens of ms per test) buys actual coverage of the
//! shell-out path. There's no point mocking `Command::output()` here.

use std::path::Path;
use std::process::Command;

/// Errors surfaced by the git shell-outs.
#[derive(Debug)]
pub enum GitError {
    /// `git` not on PATH.
    NotInstalled(String),
    /// `git` exited non-zero. Carries the command name + captured
    /// stderr for the SDK to surface verbatim.
    CommandFailed {
        command: &'static str,
        stderr: String,
    },
    /// Output couldn't be parsed against our expected shape.
    Parse(String),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::NotInstalled(msg) => write!(f, "git not available: {msg}"),
            GitError::CommandFailed { command, stderr } => {
                write!(f, "`git {command}` failed: {}", stderr.trim())
            }
            GitError::Parse(msg) => write!(f, "git output parse error: {msg}"),
        }
    }
}

impl std::error::Error for GitError {}

/// Run `git rev-parse HEAD` in `workspace_root` and return the
/// resolved 40-char (or 64-char SHA-256) SHA.
pub fn rev_parse_head(workspace_root: &Path) -> Result<String, GitError> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| GitError::NotInstalled(format!("spawn `git rev-parse HEAD`: {e}")))?;
    if !out.status.success() {
        return Err(GitError::CommandFailed {
            command: "rev-parse HEAD",
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !looks_like_sha(&sha) {
        return Err(GitError::Parse(format!(
            "expected 40- or 64-char hex SHA, got `{sha}`"
        )));
    }
    Ok(sha)
}

/// True iff `commit_sha` appears on at least one `origin/*` remote-
/// tracking branch (the SDK's push-first precheck).
///
/// Does NOT run `git fetch` first — we want the precheck to be
/// honest about the user's local state. The SDK surfaces a "push
/// your branch first" message that's actionable, not a "ran fetch
/// and still couldn't find it" indirection. Users who routinely
/// fetch via `git pull --rebase` etc. will never hit this; users
/// who didn't will see the precheck reject and run `git push`.
pub fn commit_present_on_remote(workspace_root: &Path, commit_sha: &str) -> Result<bool, GitError> {
    if !looks_like_sha(commit_sha) {
        return Err(GitError::Parse(format!(
            "commit_sha `{commit_sha}` is not a hex SHA"
        )));
    }
    let out = Command::new("git")
        .args(["branch", "-r", "--contains", commit_sha])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| GitError::NotInstalled(format!("spawn `git branch -r --contains`: {e}")))?;
    if !out.status.success() {
        // Non-zero exit usually means "no such commit" — surface as
        // the precheck-rejected case, not a command failure.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("malformed object name")
            || stderr.contains("unknown revision")
            || stderr.contains("no such commit")
            || stderr.contains("bad object")
        {
            return Ok(false);
        }
        return Err(GitError::CommandFailed {
            command: "branch -r --contains",
            stderr: stderr.into_owned(),
        });
    }
    // Each output line is a ref like `  origin/main` or `  origin/HEAD -> origin/main`.
    // We only care that at least one starts with `origin/`.
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout
        .lines()
        .map(|l| l.trim())
        .any(|l| l.starts_with("origin/")))
}

fn looks_like_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(repo: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        run_git(tmp.path(), &["init", "-q", "-b", "main"]);
        // Pinned identity so commits don't fail on CI machines without
        // a global config.
        run_git(tmp.path(), &["config", "user.email", "t@x"]);
        run_git(tmp.path(), &["config", "user.name", "t"]);
        run_git(tmp.path(), &["config", "commit.gpgsign", "false"]);
        fs::write(tmp.path().join("README"), b"x").unwrap();
        run_git(tmp.path(), &["add", "."]);
        run_git(tmp.path(), &["commit", "-q", "-m", "init"]);
        tmp
    }

    #[test]
    fn looks_like_sha_accepts_40_and_64_chars() {
        assert!(looks_like_sha(&"a".repeat(40)));
        assert!(looks_like_sha(&"f".repeat(40)));
        assert!(looks_like_sha(&"0".repeat(64)));
        assert!(!looks_like_sha("abc"));
        assert!(!looks_like_sha(&"g".repeat(40))); // non-hex
        assert!(!looks_like_sha(&"a".repeat(41))); // wrong length
    }

    #[test]
    fn rev_parse_head_returns_40_char_sha_for_fresh_repo() {
        let repo = init_repo();
        let sha = rev_parse_head(repo.path()).expect("rev-parse");
        assert!(looks_like_sha(&sha), "got: {sha}");
        assert_eq!(sha.len(), 40);
    }

    #[test]
    fn commit_present_on_remote_false_for_local_only_repo() {
        // No `origin` remote configured — push-first precheck must
        // report "not pushed" so the SDK error message kicks in.
        let repo = init_repo();
        let sha = rev_parse_head(repo.path()).unwrap();
        let present = commit_present_on_remote(repo.path(), &sha).expect("branch -r");
        assert!(!present, "local-only repo must report not-pushed");
    }

    #[test]
    fn commit_present_on_remote_true_after_simulated_push() {
        // Simulate `origin` having the commit by adding a bare
        // upstream and pushing. This mirrors the "user ran git push"
        // path; the precheck should pass.
        let repo = init_repo();
        let upstream = TempDir::new().unwrap();
        run_git(upstream.path(), &["init", "--bare", "-q"]);
        run_git(
            repo.path(),
            &["remote", "add", "origin", upstream.path().to_str().unwrap()],
        );
        run_git(repo.path(), &["push", "-q", "origin", "main"]);
        let sha = rev_parse_head(repo.path()).unwrap();
        let present = commit_present_on_remote(repo.path(), &sha).expect("branch -r");
        assert!(
            present,
            "after `git push origin main`, commit must be on origin/*"
        );
    }

    #[test]
    fn commit_present_on_remote_rejects_non_sha_input() {
        let repo = init_repo();
        let err = commit_present_on_remote(repo.path(), "not-a-sha").unwrap_err();
        assert!(matches!(err, GitError::Parse(_)));
    }

    #[test]
    fn commit_present_on_remote_false_for_unknown_sha() {
        let repo = init_repo();
        // 40-char hex that doesn't exist in the repo.
        let bogus = "0".repeat(40);
        let present = commit_present_on_remote(repo.path(), &bogus).expect("branch -r");
        assert!(!present, "unknown SHA must report not-on-remote");
    }
}
