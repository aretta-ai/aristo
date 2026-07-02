//! The flag-broken instrumentation-debt ledger:
//! `.aristo/instrumentation-debt.jsonl` — append-only JSONL, one
//! entry per `[ flag-broken ]` decision (PROPOSAL: "flag-broken =
//! recorded, visible debt").
//!
//! Committed by default (like `.aristo/canon-matches.toml`): the debt
//! record must propagate to CI and other clones so flagged accessors
//! are re-surfaced as `SKIPPED(instr-debt: <id>)` everywhere, never
//! silently green. It is therefore deliberately NOT in `aristo init`'s
//! gitignore list. JSONL keeps appends conflict-free and the history
//! auditable (re-flagging appends a new line; the FIRST line per
//! accessor is its `first_seen`).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Workspace-relative ledger path (composed under
/// `Workspace::aristo_dir()`; kept as a constant next to its only
/// writer, like `canon/catalogue.rs`'s `CATALOGUE_REL`).
pub(crate) const DEBT_LEDGER_FILE: &str = "instrumentation-debt.jsonl";

/// One recorded flag-broken decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DebtEntry {
    /// Lock-row accessor id (the ledger key).
    pub accessor_id: String,
    /// Bundle the accessor was delivered in.
    pub bundle_id: String,
    /// Bundle provenance at flag time.
    pub base_ref: String,
    pub payload_ref: String,
    /// The conformance catch that now stays visibly unprotected.
    pub catch: String,
    /// The rustc/cargo evidence observed when the accessor was
    /// flagged (E0599 headline, SUT_FEATURE_UNDECLARED line, …).
    pub evidence: String,
    /// RFC 3339 timestamp of the flag decision.
    pub flagged_at: String,
}

/// Append one entry (one JSON line) to the ledger, creating the file
/// and parent directory on first use.
pub(crate) fn append_debt_entry(path: &Path, entry: &DebtEntry) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(entry).map_err(io::Error::other)?;
    line.push('\n');
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Load the ledger keyed by `accessor_id`. The FIRST entry per
/// accessor wins (its `flagged_at` is the debt's first-seen
/// timestamp). A missing file is an empty ledger; unparsable lines
/// are skipped with a warning (an interrupted append must not brick
/// the probe).
pub(crate) fn load_debt(path: &Path) -> (BTreeMap<String, DebtEntry>, Vec<String>) {
    let mut map = BTreeMap::new();
    let mut warnings = Vec::new();
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (map, warnings),
        Err(e) => {
            warnings.push(format!("debt ledger {} unreadable: {e}", path.display()));
            return (map, warnings);
        }
    };
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<DebtEntry>(line) {
            Ok(entry) => {
                map.entry(entry.accessor_id.clone()).or_insert(entry);
            }
            Err(e) => warnings.push(format!(
                "debt ledger {}: line {} is not a valid entry ({e}); skipped",
                path.display(),
                i + 1
            )),
        }
    }
    (map, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn entry(id: &str, flagged_at: &str) -> DebtEntry {
        DebtEntry {
            accessor_id: id.into(),
            bundle_id: "turso:7b6cbae:ae85f8792372".into(),
            base_ref: "ad351877c5cf38c1fafc7f08703bfe521b8f4437".into(),
            payload_ref: "7b6cbaec04e86c0d9ac47819c77444af5054c50a".into(),
            catch: "WAL install coherence (WR-03).".into(),
            evidence: "error[E0599]: no method named `installed_snapshot` found".into(),
            flagged_at: flagged_at.into(),
        }
    }

    #[test]
    fn append_then_load_round_trips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".aristo").join(DEBT_LEDGER_FILE);
        append_debt_entry(&path, &entry("installed_snapshot", "2026-07-02T10:00:00Z")).unwrap();
        append_debt_entry(
            &path,
            &entry("inspect_header_version", "2026-07-02T10:05:00Z"),
        )
        .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            raw.lines().count(),
            2,
            "one JSON line per entry, got: {raw}"
        );
        for line in raw.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            for key in [
                "accessor_id",
                "bundle_id",
                "base_ref",
                "payload_ref",
                "catch",
                "evidence",
                "flagged_at",
            ] {
                assert!(v.get(key).is_some(), "entry must carry {key}, got: {line}");
            }
        }

        let (map, warnings) = load_debt(&path);
        assert!(warnings.is_empty(), "got: {warnings:?}");
        assert_eq!(map.len(), 2);
        assert_eq!(map["installed_snapshot"].flagged_at, "2026-07-02T10:00:00Z");
    }

    #[test]
    fn first_entry_per_accessor_is_first_seen() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(DEBT_LEDGER_FILE);
        append_debt_entry(&path, &entry("installed_snapshot", "2026-07-01T09:00:00Z")).unwrap();
        append_debt_entry(&path, &entry("installed_snapshot", "2026-07-02T10:00:00Z")).unwrap();
        let (map, _) = load_debt(&path);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map["installed_snapshot"].flagged_at, "2026-07-01T09:00:00Z",
            "re-flagging appends history but first_seen wins"
        );
    }

    #[test]
    fn missing_file_is_an_empty_ledger() {
        let tmp = TempDir::new().unwrap();
        let (map, warnings) = load_debt(&tmp.path().join("never-existed.jsonl"));
        assert!(map.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn malformed_line_is_skipped_with_a_warning() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(DEBT_LEDGER_FILE);
        append_debt_entry(&path, &entry("installed_snapshot", "2026-07-02T10:00:00Z")).unwrap();
        {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "{{ torn appen").unwrap();
        }
        let (map, warnings) = load_debt(&path);
        assert_eq!(map.len(), 1, "good entries survive a torn line");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(warnings[0].contains("line 2"), "got: {}", warnings[0]);
    }
}
