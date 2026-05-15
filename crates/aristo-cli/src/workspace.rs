//! Workspace discovery: find `aristo.toml` by walking up from a starting
//! directory.
//!
//! Aristo's notion of "workspace" matches Cargo's: the directory containing
//! `aristo.toml` is the root, and `.aristo/{index.toml, specs/, doc/}`
//! lives next to it. Walking upward from the user's cwd lets `aristo`
//! commands work from any subdirectory of a project, like `cargo` does.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    NotFound { searched_from: PathBuf },
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::NotFound { searched_from } => {
                write!(
                    f,
                    "no aristo.toml found at or above {}",
                    searched_from.display()
                )
            }
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// A located Aristo workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    /// Find the workspace by walking upward from `start` (or cwd if None)
    /// looking for an `aristo.toml`. The first ancestor directory that
    /// contains one becomes the workspace root.
    pub fn find(start: Option<&Path>) -> Result<Self, WorkspaceError> {
        let start_buf = match start {
            Some(p) => p.to_path_buf(),
            None => std::env::current_dir().expect("current_dir readable for workspace discovery"),
        };
        let mut cur: &Path = &start_buf;
        loop {
            if cur.join("aristo.toml").is_file() {
                return Ok(Workspace {
                    root: cur.to_path_buf(),
                });
            }
            cur = match cur.parent() {
                Some(p) => p,
                None => {
                    return Err(WorkspaceError::NotFound {
                        searched_from: start_buf,
                    });
                }
            };
        }
    }

    /// Path to the `.aristo/` state directory.
    pub fn aristo_dir(&self) -> PathBuf {
        self.root.join(".aristo")
    }

    /// Path to `.aristo/index.toml`.
    pub fn index_path(&self) -> PathBuf {
        self.aristo_dir().join("index.toml")
    }

    /// Path to `.aristo/specs/`.
    pub fn specs_dir(&self) -> PathBuf {
        self.aristo_dir().join("specs")
    }

    /// Path to `.aristo/doc/`.
    pub fn doc_dir(&self) -> PathBuf {
        self.aristo_dir().join("doc")
    }

    /// Path to `aristo.toml`.
    pub fn config_path(&self) -> PathBuf {
        self.root.join("aristo.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(p: &Path) {
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn finds_workspace_at_start_dir() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("aristo.toml"));
        let ws = Workspace::find(Some(tmp.path())).unwrap();
        // Compare on canonicalized form because macOS /tmp ↔ /private/tmp
        // diverges between the temp-dir handle and the walked-up parent
        // path. Workspace::find preserves the input verbatim, so we
        // canonicalize both sides for a reliable equality check.
        assert_eq!(
            ws.root.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn finds_workspace_in_ancestor() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("aristo.toml"));
        let nested = tmp.path().join("a/b/c/d");
        std::fs::create_dir_all(&nested).unwrap();
        let ws = Workspace::find(Some(&nested)).unwrap();
        assert_eq!(
            ws.root.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn errors_when_no_aristo_toml_in_chain() {
        let tmp = TempDir::new().unwrap();
        // Deliberately do NOT create aristo.toml. Walking up from a temp dir
        // shouldn't hit any aristo.toml on a clean system. (If a developer
        // has an aristo.toml at /tmp or /, this test would be misleading;
        // but that's pathological enough to ignore.)
        let nested = tmp.path().join("nope");
        std::fs::create_dir(&nested).unwrap();
        // We can't assert success-or-fail in general because of the above
        // ambiguity, so just ensure: if we DO find one, it's not the temp
        // dir we just made. (Validates that the walk reached *some* root
        // distinct from our empty start.)
        match Workspace::find(Some(&nested)) {
            Err(WorkspaceError::NotFound { .. }) => {}
            Ok(ws) => assert_ne!(
                ws.root, nested,
                "walk should not stop in our empty temp dir"
            ),
        }
    }

    #[test]
    fn aristo_dir_paths_compose_correctly() {
        let ws = Workspace {
            root: PathBuf::from("/proj"),
        };
        assert_eq!(ws.aristo_dir(), PathBuf::from("/proj/.aristo"));
        assert_eq!(ws.index_path(), PathBuf::from("/proj/.aristo/index.toml"));
        assert_eq!(ws.specs_dir(), PathBuf::from("/proj/.aristo/specs"));
        assert_eq!(ws.doc_dir(), PathBuf::from("/proj/.aristo/doc"));
        assert_eq!(ws.config_path(), PathBuf::from("/proj/aristo.toml"));
    }
}
