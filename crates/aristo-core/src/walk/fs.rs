//! Filesystem walker — find every `.rs` file under a project root and
//! extract annotations from each.
//!
//! Built-in ignore set excludes the directories that are never source: build
//! output (`target/`), version control (`.git/`), Aristo state
//! (`.aristo/`), and Node modules (`node_modules/`). Adding more ignore
//! roots is a `WalkOptions` field future slices extend; defaults stay
//! minimal so the common case "Cargo project at the workspace root" needs
//! no configuration.
//!
//! Output paths are returned relative to the walk root, in lexicographic
//! order, so `aristo index` writes a deterministic `.aristo/index.toml`
//! across runs and across machines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::walk::extract::{extract_from_source, ExtractError, ExtractedAnnotation};

/// One annotation discovered during a filesystem walk. Wraps
/// [`ExtractedAnnotation`] with the file path (relative to the walk root)
/// the annotation was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAnnotation {
    pub file: PathBuf,
    pub annotation: ExtractedAnnotation,
}

#[derive(Debug, thiserror::Error)]
pub enum FsWalkError {
    #[error("walk root does not exist or is not a directory: {0}")]
    BadRoot(PathBuf),
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: ExtractError,
    },
}

/// Default directory names to skip during the walk. Matched against each
/// directory entry's basename (not the full path), so `vendor/target/` is
/// also skipped.
const DEFAULT_IGNORED_DIRS: &[&str] = &["target", ".git", ".aristo", "node_modules"];

/// Walk `root` recursively, parse every `.rs` file, and collect the
/// annotations they contain.
///
/// Errors during file IO or parse abort the walk and surface the offending
/// path. A successful walk returns annotations grouped by file (alphabetical
/// path order), and within each file in source order.
#[aristo::intent(
    "walk_directory returns paths RELATIVE to root, in stable lexicographic \
     order — same input directory must yield byte-identical output across \
     runs and across machines so .aristo/index.toml stays deterministic.",
    verify = "test",
    id = "walk_directory_is_deterministic"
)]
pub fn walk_directory(root: &Path) -> Result<Vec<DiscoveredAnnotation>, FsWalkError> {
    if !root.is_dir() {
        return Err(FsWalkError::BadRoot(root.to_path_buf()));
    }

    let mut by_file: BTreeMap<PathBuf, Vec<ExtractedAnnotation>> = BTreeMap::new();

    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name() // deterministic dir-traversal order
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e));

    for entry in walker {
        let entry = entry.map_err(|e| FsWalkError::Io {
            path: e
                .path()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf()),
            source: e
                .into_io_error()
                .unwrap_or_else(|| std::io::Error::other("walkdir error without underlying io")),
        })?;

        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }

        let abs_path = entry.path();
        let source = std::fs::read_to_string(abs_path).map_err(|source| FsWalkError::Io {
            path: abs_path.to_path_buf(),
            source,
        })?;
        let annotations = extract_from_source(&source).map_err(|source| FsWalkError::Parse {
            path: abs_path.to_path_buf(),
            source,
        })?;

        if annotations.is_empty() {
            continue;
        }

        let rel = abs_path
            .strip_prefix(root)
            .unwrap_or(abs_path)
            .to_path_buf();
        by_file.insert(rel, annotations);
    }

    let mut out = Vec::new();
    for (file, annotations) in by_file {
        for annotation in annotations {
            out.push(DiscoveredAnnotation {
                file: file.clone(),
                annotation,
            });
        }
    }
    Ok(out)
}

fn is_ignored_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };
    DEFAULT_IGNORED_DIRS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn finds_annotations_across_multiple_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "src/a.rs",
            r#"#[aristo::intent("from a")] fn a() {}"#,
        );
        write(
            tmp.path(),
            "src/b.rs",
            r#"#[aristo::intent("from b")] fn b() {}"#,
        );

        let found = walk_directory(tmp.path()).unwrap();
        assert_eq!(found.len(), 2);
        // Lexicographic file order: a.rs, then b.rs.
        assert_eq!(found[0].file, PathBuf::from("src/a.rs"));
        assert_eq!(found[1].file, PathBuf::from("src/b.rs"));
        assert_eq!(found[0].annotation.text, "from a");
        assert_eq!(found[1].annotation.text, "from b");
    }

    #[test]
    fn skips_target_and_git_and_aristo_directories() {
        let tmp = tempfile::tempdir().unwrap();
        // Build artifact masquerading as Rust — must be skipped.
        write(
            tmp.path(),
            "target/debug/build.rs",
            r#"#[aristo::intent("would be wrong to find")] fn x() {}"#,
        );
        // Git internal — must be skipped.
        write(
            tmp.path(),
            ".git/hooks/post-commit.rs",
            r#"#[aristo::intent("git internal")] fn x() {}"#,
        );
        // Aristo state — must be skipped.
        write(
            tmp.path(),
            ".aristo/scratch.rs",
            r#"#[aristo::intent("scratch")] fn x() {}"#,
        );
        // Node modules — must be skipped.
        write(
            tmp.path(),
            "node_modules/lib.rs",
            r#"#[aristo::intent("vendored")] fn x() {}"#,
        );
        // Real source — must be found.
        write(
            tmp.path(),
            "src/lib.rs",
            r#"#[aristo::intent("real source")] fn x() {}"#,
        );

        let found = walk_directory(tmp.path()).unwrap();
        assert_eq!(found.len(), 1, "only src/lib.rs should be found");
        assert_eq!(found[0].annotation.text, "real source");
    }

    #[test]
    fn returns_empty_for_dir_with_no_rust_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "README.md", "# hello");
        write(tmp.path(), "src/x.txt", "not rust");
        assert!(walk_directory(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn returns_empty_for_dir_with_rust_files_but_no_annotations() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/main.rs", "fn main() {}");
        assert!(walk_directory(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn errors_on_nonexistent_root() {
        let nope = std::env::temp_dir().join("definitely-not-here-aristo-test");
        assert!(matches!(
            walk_directory(&nope),
            Err(FsWalkError::BadRoot(_))
        ));
    }

    #[test]
    fn errors_on_unparseable_rust_with_path_in_message() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/broken.rs", "fn unbalanced(");
        match walk_directory(tmp.path()) {
            Err(FsWalkError::Parse { path, .. }) => {
                assert!(path.ends_with("broken.rs"), "got: {}", path.display());
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn deeply_nested_files_are_found() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "crates/foo/src/lib.rs",
            r#"#[aristo::intent("nested")] fn x() {}"#,
        );
        let found = walk_directory(tmp.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, PathBuf::from("crates/foo/src/lib.rs"));
    }

    #[test]
    fn output_is_byte_identical_across_runs() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "src/a.rs",
            r#"#[aristo::intent("a")] fn a() {} #[aristo::intent("a2")] fn a2() {}"#,
        );
        write(
            tmp.path(),
            "src/sub/c.rs",
            r#"#[aristo::intent("c")] fn c() {}"#,
        );
        write(
            tmp.path(),
            "src/b.rs",
            r#"#[aristo::intent("b")] fn b() {}"#,
        );

        let r1 = walk_directory(tmp.path()).unwrap();
        let r2 = walk_directory(tmp.path()).unwrap();
        assert_eq!(r1, r2, "two walks of the same tree must match exactly");
    }
}
