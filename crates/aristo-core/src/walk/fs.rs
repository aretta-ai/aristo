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

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::IndexConfig;
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
    #[error("invalid exclude pattern `{pattern}`: {source}")]
    BadPattern {
        pattern: String,
        #[source]
        source: globset::Error,
    },
}

/// Options applied to every walk: in addition to the hardcoded default
/// directory skip set, callers can supply glob patterns (matched
/// relative to the walk root, forward-slash style) that prune
/// individual files or whole subtrees from consideration.
#[derive(Debug, Default, Clone)]
pub struct WalkOptions {
    excludes: GlobSet,
}

impl WalkOptions {
    /// Empty options — no extra excludes; equivalent to `Default`.
    pub fn none() -> Self {
        Self::default()
    }

    /// Build options from a `[index]` config section.
    pub fn from_index_config(cfg: &IndexConfig) -> Result<Self, FsWalkError> {
        Self::from_patterns(&cfg.exclude)
    }

    /// Build options from raw glob pattern strings.
    pub fn from_patterns<S: AsRef<str>>(patterns: &[S]) -> Result<Self, FsWalkError> {
        let mut builder = GlobSetBuilder::new();
        for p in patterns {
            let p = p.as_ref();
            let glob = Glob::new(p).map_err(|e| FsWalkError::BadPattern {
                pattern: p.to_string(),
                source: e,
            })?;
            builder.add(glob);
        }
        let excludes = builder.build().map_err(|e| FsWalkError::BadPattern {
            pattern: "<set>".to_string(),
            source: e,
        })?;
        Ok(Self { excludes })
    }

    /// True iff `rel` (a path relative to the walk root) matches any
    /// configured exclude pattern. `false` if no excludes are
    /// configured. Public so non-walker code (e.g. `aristo lint --fix`,
    /// which has its own file-iteration loop) can apply the same
    /// filter consistently.
    #[aristo::intent(
        "Path components are joined with forward slashes before glob \
         matching, so the same aristo.toml exclude list works on POSIX \
         and Windows. A `rel.to_str()` shortcut would feed `\\`-separated \
         paths into globset on Windows and silently make patterns like \
         `**/tests/ui/**` never match — failing open, not closed, which \
         would index files the user thought were excluded.",
        verify = "neural",
        id = "walk_excludes_normalize_to_forward_slash"
    )]
    pub fn excludes_path(&self, rel: &Path) -> bool {
        if self.excludes.is_empty() {
            return false;
        }
        // Normalize to forward slashes so the same glob works on both
        // Windows and POSIX hosts.
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        self.excludes.is_match(rel_str)
    }
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
    "The same source tree yields byte-identical results across runs and \
     machines: lexicographic path order, source order within each file. \
     Parallelism or unsorted directory reads would silently break the \
     index's reproducibility guarantee.",
    verify = "test",
    id = "walk_directory_is_deterministic"
)]
pub fn walk_directory(root: &Path) -> Result<Vec<DiscoveredAnnotation>, FsWalkError> {
    walk_directory_with(root, &WalkOptions::none())
}

/// Same as [`walk_directory`] but honors the supplied `opts` (project
/// excludes) on top of the hardcoded default-skipped set.
pub fn walk_directory_with(
    root: &Path,
    opts: &WalkOptions,
) -> Result<Vec<DiscoveredAnnotation>, FsWalkError> {
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
        let rel_for_glob = abs_path.strip_prefix(root).unwrap_or(abs_path);
        if opts.excludes_path(rel_for_glob) {
            continue;
        }
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

/// Lightweight walk: collect every `.rs` file path under `root` honoring
/// the same skip set as [`walk_directory`], but WITHOUT reading or
/// parsing the files. Used by the J5 freshness preflight, where only
/// filesystem metadata (mtime) matters and parsing every file would be
/// wasted work.
///
/// Returns absolute paths so callers can stat them directly. Path order
/// is lexicographic for determinism.
pub fn walk_for_freshness(root: &Path) -> Result<Vec<PathBuf>, FsWalkError> {
    walk_for_freshness_with(root, &WalkOptions::none())
}

/// Same as [`walk_for_freshness`] but honors the supplied `opts`.
pub fn walk_for_freshness_with(
    root: &Path,
    opts: &WalkOptions,
) -> Result<Vec<PathBuf>, FsWalkError> {
    if !root.is_dir() {
        return Err(FsWalkError::BadRoot(root.to_path_buf()));
    }
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
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
        let rel_for_glob = entry.path().strip_prefix(root).unwrap_or(entry.path());
        if opts.excludes_path(rel_for_glob) {
            continue;
        }
        out.push(entry.path().to_path_buf());
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

    // ─── WalkOptions / exclude globs ─────────────────────────────────────

    #[test]
    fn exclude_glob_skips_matching_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "src/lib.rs",
            r#"#[aristo::intent("keep")] fn k() {}"#,
        );
        write(
            tmp.path(),
            "tests/ui/fail/empty_text.rs",
            r#"#[aristo::intent("")] fn drop() {}"#,
        );

        let opts = WalkOptions::from_patterns(&["**/tests/ui/**"]).unwrap();
        let r = walk_directory_with(tmp.path(), &opts).unwrap();
        assert_eq!(r.len(), 1, "trybuild fixture must be excluded");
        assert_eq!(r[0].annotation.text, "keep");
    }

    #[test]
    fn exclude_glob_applies_to_freshness_walk() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "fn a() {}");
        write(tmp.path(), "tests/ui/fail/x.rs", "fn b() {}");

        let opts = WalkOptions::from_patterns(&["**/tests/ui/**"]).unwrap();
        let paths = walk_for_freshness_with(tmp.path(), &opts).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("src/lib.rs"));
    }

    #[test]
    fn empty_options_walks_everything() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "src/a.rs",
            r#"#[aristo::intent("a")] fn a() {}"#,
        );
        write(
            tmp.path(),
            "tests/b.rs",
            r#"#[aristo::intent("b")] fn b() {}"#,
        );

        let r = walk_directory_with(tmp.path(), &WalkOptions::none()).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn bad_glob_pattern_surfaces_error() {
        // Unterminated character class → globset rejects.
        let result = WalkOptions::from_patterns(&["src/[unterminated"]);
        assert!(matches!(result, Err(FsWalkError::BadPattern { .. })));
    }

    #[test]
    fn excludes_compose_with_default_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "src/keep.rs",
            r#"#[aristo::intent("keep")] fn k() {}"#,
        );
        // target/ is always-skipped; this entry must not appear regardless
        // of opts.
        write(
            tmp.path(),
            "target/debug/build.rs",
            r#"#[aristo::intent("never")] fn n() {}"#,
        );
        // tests/fixtures is excluded via opts.
        write(
            tmp.path(),
            "tests/fixtures/bad.rs",
            r#"#[aristo::intent("")] fn d() {}"#,
        );

        let opts = WalkOptions::from_patterns(&["tests/fixtures/**"]).unwrap();
        let r = walk_directory_with(tmp.path(), &opts).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].annotation.text, "keep");
    }
}
