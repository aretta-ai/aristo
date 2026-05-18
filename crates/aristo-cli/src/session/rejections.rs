//! Append-only JSONL log of rejected items across all closed sessions.
//!
//! When a user rejects an item during a review session, the rejection
//! is appended here so future sessions of the same kind can recognize
//! "this is the same suggestion the user already rejected" and route
//! the item to the auto-rejected menu instead of the main flow
//! (design doc D7).
//!
//! Wire format: one JSON object per line, newline-terminated. Append
//! is open-write-line — fast path for the common case (single writer,
//! no concurrent rejection log churn). The file is gitignored so
//! cross-team contamination is not a concern.
//!
//! The `fingerprint` field is per-kind opaque structured data; the
//! per-kind `matches_prior_rejection` callback knows how to interpret
//! it. The substrate stores it as `serde_json::Value` and does not
//! introspect.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};

use crate::session::types::ItemRef;
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// One rejection record. Serialized as a single JSONL line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectionEntry {
    /// RFC3339 timestamp the rejection was recorded.
    pub ts: String,
    /// Session kind that produced the rejection (e.g.
    /// `"critique-review"`). Future sessions of OTHER kinds ignore
    /// this entry when computing the auto-rejected set.
    pub kind: String,
    /// Opaque per-kind item reference.
    pub item_ref: ItemRef,
    /// Optional free-text reason captured during the rejection prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Per-kind structured fingerprint used by `matches_prior_rejection`
    /// to recognize equivalent future suggestions. Substrate-opaque.
    pub fingerprint: serde_json::Value,
}

#[aristo::intent(
    "Rejection-log writes use `OpenOptions::append(true).create(true)` \
     plus a single `write_all` of the JSON line + `\\n`. No locking is \
     needed because (a) the file is per-workspace and gitignored, so \
     no cross-team writers; (b) writes are line-sized and POSIX \
     guarantees write atomicity for buffers ≤ PIPE_BUF (4 KiB on \
     Linux/macOS, well above any single JSON rejection record). A \
     refactor that read-modify-wrote the whole file would lose this \
     property and need explicit locking.",
    verify = "neural",
    id = "rejection_log_append_relies_on_posix_write_atomicity"
)]
pub fn append(ws: &Workspace, entry: &RejectionEntry) -> CliResult<()> {
    let dir = ws.sessions_dir();
    std::fs::create_dir_all(&dir)?;
    let path = ws.sessions_rejections_log();
    let line = serde_json::to_string(entry).map_err(|e| CliError::Other {
        message: format!("rejection serialize: {e}"),
        exit_code: 1,
    })?;
    let mut f = OpenOptions::new().append(true).create(true).open(&path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Read every rejection currently logged. Missing file is treated as
/// empty (no rejections yet). Parse errors on individual lines skip
/// the line and continue — a hand-edited rejections.log shouldn't
/// crash a review session.
pub fn read_all(ws: &Workspace) -> CliResult<Vec<RejectionEntry>> {
    let path = ws.sessions_rejections_log();
    let f = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for line in BufReader::new(f).lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<RejectionEntry>(trimmed) {
            out.push(entry);
        }
        // Bad lines silently skipped — see fn-level docstring.
    }
    Ok(out)
}

/// Filter [`read_all`] by kind. Used by per-kind `matches_prior_rejection`
/// computation at session start to limit the comparison set.
pub fn read_for_kind(ws: &Workspace, kind: &str) -> CliResult<Vec<RejectionEntry>> {
    Ok(read_all(ws)?
        .into_iter()
        .filter(|r| r.kind == kind)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_workspace(dir: &TempDir) -> Workspace {
        std::fs::write(dir.path().join("aristo.toml"), "").unwrap();
        Workspace {
            root: dir.path().to_path_buf(),
        }
    }

    fn fixture(kind: &str, ref_str: &str) -> RejectionEntry {
        RejectionEntry {
            ts: "2026-05-18T13:05:00Z".into(),
            kind: kind.into(),
            item_ref: ItemRef::from_opaque(ref_str),
            note: Some("intentionally narrative".into()),
            fingerprint: serde_json::json!({
                "category": "clarity",
                "rationale_sketch": "defensive_commentary",
            }),
        }
    }

    #[test]
    fn append_then_read_round_trips_one_entry() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let e = fixture("critique-review", "foo#0");
        append(&ws, &e).unwrap();
        let back = read_all(&ws).unwrap();
        assert_eq!(back, vec![e]);
    }

    #[test]
    fn append_is_append_only_preserving_prior_entries() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let a = fixture("critique-review", "foo#0");
        let b = fixture("critique-review", "bar#1");
        let c = fixture("proof-review", "proof_x#verdict");
        append(&ws, &a).unwrap();
        append(&ws, &b).unwrap();
        append(&ws, &c).unwrap();
        let all = read_all(&ws).unwrap();
        assert_eq!(all, vec![a, b, c]);
    }

    #[test]
    fn read_for_kind_filters_to_matching_entries() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let a = fixture("critique-review", "foo#0");
        let b = fixture("proof-review", "proof#verdict");
        let c = fixture("critique-review", "baz#2");
        append(&ws, &a).unwrap();
        append(&ws, &b).unwrap();
        append(&ws, &c).unwrap();
        let critique = read_for_kind(&ws, "critique-review").unwrap();
        assert_eq!(critique, vec![a, c]);
    }

    #[test]
    fn read_all_returns_empty_when_log_missing() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        assert!(read_all(&ws).unwrap().is_empty());
    }

    #[test]
    fn read_all_skips_garbage_lines_without_erroring() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let good = fixture("critique-review", "foo#0");
        append(&ws, &good).unwrap();
        // Append garbage that a hand-edit might produce.
        let mut f = OpenOptions::new()
            .append(true)
            .open(ws.sessions_rejections_log())
            .unwrap();
        f.write_all(b"this is not json\n").unwrap();
        f.write_all(b"\n").unwrap(); // blank line — also tolerated
        let back = read_all(&ws).unwrap();
        assert_eq!(back, vec![good]);
    }
}
