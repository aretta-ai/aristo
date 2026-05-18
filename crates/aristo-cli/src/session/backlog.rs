//! Per-kind backlog of items deferred from prior sessions.
//!
//! Two paths feed the backlog:
//!
//! 1. **`session exit --defer-undecided`** — every still-open item at
//!    close-time gets moved here so the next session can pick them up.
//! 2. **`session decide --bucket pending`** — explicit per-item defer
//!    during an active session.
//!
//! Backlog items NEVER silently vanish. They surface in two places:
//! the opening menu of each new session (per design doc D7's
//! `backlog` population) and the `aristo status` summary line (D9).
//!
//! Wire format: one TOML file per kind at `backlog/<kind>.toml`.
//! Reading + writing goes through the atomic-write helper so a
//! concurrent reader cannot see partial state.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::commands::index::atomic_write;
use crate::session::types::{ItemRef, SessionId};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// One deferred item parked for later review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklogEntry {
    /// Opaque per-kind item reference.
    pub item_ref: ItemRef,
    /// RFC3339 timestamp the item entered the backlog.
    pub deferred_at: String,
    /// Session id the item was deferred FROM (audit breadcrumb).
    pub deferred_from_session: SessionId,
    /// Optional free-text reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Per-kind opaque payload (e.g. for critique: the finding body
    /// snapshot, so future review can render it without re-running
    /// the worker).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

/// On-disk shape for `backlog/<kind>.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklogFile {
    pub schema_version: u32,
    #[serde(default)]
    pub items: Vec<BacklogEntry>,
}

fn backlog_path(ws: &Workspace, kind: &str) -> PathBuf {
    ws.sessions_backlog_dir().join(format!("{kind}.toml"))
}

/// Read the backlog for one kind. Missing file → empty backlog.
pub fn read(ws: &Workspace, kind: &str) -> CliResult<BacklogFile> {
    let path = backlog_path(ws, kind);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(BacklogFile {
                schema_version: 1,
                items: Vec::new(),
            });
        }
        Err(e) => return Err(e.into()),
    };
    toml::from_str(&text).map_err(|e| CliError::Other {
        message: format!("backlog parse {}: {e}", path.display()),
        exit_code: 1,
    })
}

#[aristo::intent(
    "Backlog writes go through `atomic_write` (temp-file + rename) so a \
     concurrent reader cannot observe a partially-serialized file. The \
     backlog is the only durable record of deferred items between \
     sessions; a partial write that deserialize-fails would look like \
     'no backlog' to a reader and silently drop user-deferred items — \
     the exact failure mode the substrate exists to prevent.",
    verify = "neural",
    id = "backlog_writes_are_atomic_via_tempfile_rename"
)]
pub fn write(ws: &Workspace, kind: &str, backlog: &BacklogFile) -> CliResult<()> {
    fs::create_dir_all(ws.sessions_backlog_dir())?;
    let path = backlog_path(ws, kind);
    let toml_text = toml::to_string(backlog).map_err(|e| CliError::Other {
        message: format!("backlog serialize: {e}"),
        exit_code: 1,
    })?;
    atomic_write(&path, &toml_text)
}

#[aristo::intent(
    "Appending to the backlog is read-modify-write: read the existing \
     file, push the new entry, atomic-write the result. The `pending` \
     bucket NEVER silently drops — every deferred item must land in \
     this file. A refactor that used `OpenOptions::append` would lose \
     atomicity (the backlog is TOML, not line-delimited; an interrupted \
     append produces a non-parseable file).",
    verify = "neural",
    id = "backlog_append_is_read_modify_write_via_atomic"
)]
pub fn append_entry(ws: &Workspace, kind: &str, entry: BacklogEntry) -> CliResult<()> {
    let mut backlog = read(ws, kind)?;
    backlog.schema_version = 1;
    backlog.items.push(entry);
    write(ws, kind, &backlog)
}

/// Count current backlog items for one kind. Used by `aristo status`
/// to surface the backlog without enumerating it.
pub fn count(ws: &Workspace, kind: &str) -> CliResult<usize> {
    Ok(read(ws, kind)?.items.len())
}

#[aristo::intent(
    "Draining the backlog atomically removes the file after returning \
     its contents — once the caller has the items, the file is gone. \
     The pattern matches `aristo verify --apply-verdicts`: read all, \
     mutate, the artifact is consumed. A refactor that read without \
     deleting would surface the same backlog items every session start, \
     creating zombie-deferral. A refactor that deleted before returning \
     would lose data if the caller crashed mid-handle — read-then-delete \
     keeps the items in memory while the file is gone.",
    verify = "neural",
    id = "drain_returns_items_then_deletes_file"
)]
pub fn drain(ws: &Workspace, kind: &str) -> CliResult<Vec<BacklogEntry>> {
    let backlog = read(ws, kind)?;
    let items = backlog.items;
    let path = backlog_path(ws, kind);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(items)
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

    fn fixture(ref_str: &str) -> BacklogEntry {
        BacklogEntry {
            item_ref: ItemRef::from_opaque(ref_str),
            deferred_at: "2026-05-18T13:10:00Z".into(),
            deferred_from_session: SessionId::from_string("01J5K9N7".into()),
            note: None,
            data: serde_json::Value::Null,
        }
    }

    #[test]
    fn read_returns_empty_when_missing() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let b = read(&ws, "critique-review").unwrap();
        assert!(b.items.is_empty());
    }

    #[test]
    fn append_then_read_round_trips() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let a = fixture("foo#0");
        let b = fixture("bar#1");
        append_entry(&ws, "critique-review", a.clone()).unwrap();
        append_entry(&ws, "critique-review", b.clone()).unwrap();
        let back = read(&ws, "critique-review").unwrap();
        assert_eq!(back.schema_version, 1);
        assert_eq!(back.items, vec![a, b]);
    }

    #[test]
    fn count_matches_item_total() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        assert_eq!(count(&ws, "critique-review").unwrap(), 0);
        append_entry(&ws, "critique-review", fixture("a#0")).unwrap();
        append_entry(&ws, "critique-review", fixture("b#0")).unwrap();
        append_entry(&ws, "critique-review", fixture("c#0")).unwrap();
        assert_eq!(count(&ws, "critique-review").unwrap(), 3);
    }

    #[test]
    fn drain_returns_items_then_clears_file() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        append_entry(&ws, "critique-review", fixture("a#0")).unwrap();
        append_entry(&ws, "critique-review", fixture("b#0")).unwrap();
        let drained = drain(&ws, "critique-review").unwrap();
        assert_eq!(drained.len(), 2);
        // File should be gone — second drain returns empty.
        let drained2 = drain(&ws, "critique-review").unwrap();
        assert!(drained2.is_empty());
        assert!(!backlog_path(&ws, "critique-review").exists());
    }

    #[test]
    fn drain_when_missing_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let drained = drain(&ws, "critique-review").unwrap();
        assert!(drained.is_empty());
    }

    #[test]
    fn distinct_kinds_have_distinct_backlogs() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        append_entry(&ws, "critique-review", fixture("c#0")).unwrap();
        append_entry(&ws, "proof-review", fixture("p#0")).unwrap();
        append_entry(&ws, "proof-review", fixture("q#0")).unwrap();
        assert_eq!(count(&ws, "critique-review").unwrap(), 1);
        assert_eq!(count(&ws, "proof-review").unwrap(), 2);
    }
}
