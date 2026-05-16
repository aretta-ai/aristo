//! J5 freshness preflight — shared across every read command.
//!
//! Compares per-file source mtimes against the index file's mtime. If
//! any source file is newer, emits one advisory line to stderr
//! recommending `aristo stamp`. Exit code unchanged: the warning is
//! advisory, not a gate.
//!
//! Refresh commands (`aristo stamp`, `aristo index`) do NOT call this —
//! they ARE the refresh path; emitting "stale" before regenerating
//! would be misleading.

use std::fs;
use std::time::SystemTime;

use aristo_core::walk::walk_for_freshness;

use crate::Workspace;

/// Result of one freshness check.
#[derive(Debug, Clone)]
pub(crate) struct FreshnessReport {
    /// Number of source files whose mtime exceeds the index mtime.
    pub stale_count: usize,
    /// True if `.aristo/index.toml` is absent (no comparison possible).
    /// Callers usually surface their own "no index" error before reaching
    /// preflight, so this is mostly for callers that want to short-circuit.
    /// Currently exposed for test introspection and future callers (e.g.
    /// `aristo verify --audit-only`) that may treat it specially.
    #[allow(dead_code)]
    pub index_missing: bool,
}

impl FreshnessReport {
    pub(crate) fn is_stale(&self) -> bool {
        self.stale_count > 0
    }
}

#[aristo::intent(
    "If any source file's mtime is newer than the index file's mtime, \
     the index is stale relative to source. The comparison is one-shot \
     per command invocation; no caching, no incremental tracking — \
     correctness over speed for an advisory check.",
    verify = "neural",
    id = "freshness_check_compares_source_mtimes_to_index"
)]
pub(crate) fn freshness_check(ws: &Workspace) -> FreshnessReport {
    let index_path = ws.index_path();
    let Ok(index_meta) = fs::metadata(&index_path) else {
        return FreshnessReport {
            stale_count: 0,
            index_missing: true,
        };
    };
    let index_mtime = index_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    let sources = walk_for_freshness(&ws.root).unwrap_or_default();
    let stale_count = sources
        .into_iter()
        .filter(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .map(|src_mtime| src_mtime > index_mtime)
                .unwrap_or(false)
        })
        .count();

    FreshnessReport {
        stale_count,
        index_missing: false,
    }
}

/// Emit the standard J5 advisory line to stderr if the report shows
/// stale source. No-op otherwise. Callers run this before doing their
/// own read of the index.
pub(crate) fn emit_advisory_if_stale(report: &FreshnessReport) {
    if !report.is_stale() {
        return;
    }
    eprintln!(
        "warning: .aristo/index.toml may be stale relative to source ({} files newer than indexed).",
        report.stale_count
    );
    eprintln!("         Run `aristo stamp` to refresh.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_ws() -> (TempDir, Workspace) {
        let tmp = TempDir::new().unwrap();
        // aristo init creates the marker file
        fs::write(
            tmp.path().join("aristo.toml"),
            "[verify]\ndefault_method = \"test\"\n",
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join(".aristo")).unwrap();
        // Empty index marker for the freshness test to have something to compare
        fs::write(
            tmp.path().join(".aristo/index.toml"),
            "[__meta__]\nschema_version = 1\n",
        )
        .unwrap();
        let ws = Workspace::find(Some(tmp.path())).unwrap();
        (tmp, ws)
    }

    #[test]
    fn fresh_index_reports_zero_stale() {
        let (_tmp, ws) = make_ws();
        // No source files exist — trivially fresh.
        let report = freshness_check(&ws);
        assert_eq!(report.stale_count, 0);
        assert!(!report.is_stale());
        assert!(!report.index_missing);
    }

    #[test]
    fn source_newer_than_index_reports_stale() {
        let (tmp, ws) = make_ws();
        sleep(Duration::from_millis(50));
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "fn x() {}").unwrap();

        let report = freshness_check(&ws);
        assert_eq!(report.stale_count, 1);
        assert!(report.is_stale());
    }

    #[test]
    fn missing_index_reports_index_missing_not_stale() {
        let (tmp, ws) = make_ws();
        fs::remove_file(tmp.path().join(".aristo/index.toml")).unwrap();

        let report = freshness_check(&ws);
        assert!(report.index_missing);
        assert_eq!(report.stale_count, 0);
    }

    #[test]
    fn multiple_stale_sources_count_correctly() {
        let (tmp, ws) = make_ws();
        sleep(Duration::from_millis(50));
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/a.rs"), "fn a() {}").unwrap();
        fs::write(tmp.path().join("src/b.rs"), "fn b() {}").unwrap();
        fs::write(tmp.path().join("src/c.rs"), "fn c() {}").unwrap();

        let report = freshness_check(&ws);
        assert_eq!(report.stale_count, 3);
    }
}
