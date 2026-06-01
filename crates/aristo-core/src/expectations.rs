//! `.aristo/expectations.toml` — the user-side known-failure waiver
//! sidecar (Phase 16 (c)).
//!
//! Records property gaps the user has explicitly accepted: "my code
//! currently violates this canon property, and I acknowledge it." The
//! *property check* is Aretta's; the *acknowledgment that this repo
//! currently fails it* is the user's, so it lives here in their repo —
//! git-tracked, hand-readable, and reviewable in a diff.
//!
//! ## Shape (mirrors the other `.aristo/` sidecars)
//!
//! ```toml
//! [__meta__]
//! schema_version = 1
//!
//! ["aristos:wal_initialized_reflects_sync_outcome"]
//! reason = "turso reports initialized from a file-existence proxy; tracked upstream"
//! tracking = "https://github.com/tursodatabase/turso/issues/1234"   # optional
//! accepted_at = "2026-06-01T12:34:56Z"
//! ```
//!
//! ## Why key on the stable canon id, not the opaque `aret_*`
//!
//! The opaque id churns on every `aristo stamp` / rebind. The stable
//! prefixed canon id (`aristos:…` / `kanon:…`) survives re-stamps, so a
//! waiver written today is still matched after the next stamp with zero
//! migration logic.
//!
//! ## Ownership
//!
//! `aristo stamp` never touches this file (unlike `index.toml`, which is
//! machine-regenerated). It is written only by `aristo verify --accept`
//! and read at verify time as a join against the live results.

use std::collections::BTreeMap;
use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::index::AnnotationId;

/// The parsed `.aristo/expectations.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExpectationsFile {
    #[serde(rename = "__meta__", default)]
    pub meta: ExpectationsMeta,

    /// `canon_id → Expectation`. Keyed on the **stable prefixed canon
    /// id** (`aristos:foo` / `kanon:bar`). `BTreeMap` for deterministic
    /// serialization — this file is committed, so a stable order keeps
    /// git diffs clean.
    #[serde(flatten)]
    pub entries: BTreeMap<AnnotationId, Expectation>,
}

/// `[__meta__]` header. Matches the `index.toml` / `canon-matches.toml`
/// convention so all sidecars read the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectationsMeta {
    pub schema_version: u32,
}

impl Default for ExpectationsMeta {
    fn default() -> Self {
        Self { schema_version: 1 }
    }
}

/// One accepted gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expectation {
    /// Why the gap is accepted — recorded verbatim and shown on the
    /// verify card. Mandatory: a reasonless waiver is how baselines rot.
    pub reason: String,
    /// Optional tracking reference (issue URL, ticket id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<String>,
    /// RFC3339 timestamp the gap was accepted.
    pub accepted_at: String,
}

impl ExpectationsFile {
    /// Read + parse the file. A **missing** file is not an error — it
    /// means "no gaps accepted" and returns the default (empty). A
    /// malformed file propagates a parse error so a hand-edit typo
    /// surfaces loudly rather than silently dropping the user's waivers.
    pub fn read(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw)
                .map_err(|e| io::Error::other(format!("parse {}: {e}", path.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Serialize + write atomically (temp-then-rename), creating parent
    /// dirs as needed. Mirrors the other sidecars' write path.
    pub fn write_atomic(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::other(format!("serialize expectations: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, toml_text.as_bytes())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The accepted gap for `id`, if any.
    pub fn get(&self, id: &AnnotationId) -> Option<&Expectation> {
        self.entries.get(id)
    }

    /// True iff `id` has an accepted gap.
    pub fn is_waived(&self, id: &AnnotationId) -> bool {
        self.entries.contains_key(id)
    }

    /// Record (or replace) an accepted gap. Idempotent: re-accepting the
    /// same id overwrites the reason / tracking / timestamp rather than
    /// accumulating duplicates.
    pub fn accept(
        &mut self,
        id: AnnotationId,
        reason: String,
        tracking: Option<String>,
        accepted_at: String,
    ) {
        self.entries.insert(
            id,
            Expectation {
                reason,
                tracking,
                accepted_at,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    #[test]
    fn missing_file_reads_as_empty_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".aristo/expectations.toml");
        let f = ExpectationsFile::read(&path).unwrap();
        assert_eq!(f, ExpectationsFile::default());
        assert!(f.entries.is_empty());
        assert_eq!(f.meta.schema_version, 1);
    }

    #[test]
    fn accept_then_write_then_read_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".aristo/expectations.toml");

        let mut f = ExpectationsFile::default();
        f.accept(
            id("aristos:wal_initialized_reflects_sync_outcome"),
            "turso uses a file-existence proxy".into(),
            Some("https://example/issues/1".into()),
            "2026-06-01T12:34:56Z".into(),
        );
        f.write_atomic(&path).unwrap();

        let back = ExpectationsFile::read(&path).unwrap();
        assert_eq!(back, f);

        let exp = back
            .get(&id("aristos:wal_initialized_reflects_sync_outcome"))
            .expect("entry present");
        assert_eq!(exp.reason, "turso uses a file-existence proxy");
        assert_eq!(exp.tracking.as_deref(), Some("https://example/issues/1"));
        assert!(back.is_waived(&id("aristos:wal_initialized_reflects_sync_outcome")));
    }

    #[test]
    fn serialized_form_has_meta_header_and_prefixed_key() {
        let mut f = ExpectationsFile::default();
        f.accept(
            id("aristos:foo"),
            "because reasons".into(),
            None,
            "2026-06-01T00:00:00Z".into(),
        );
        let text = toml::to_string_pretty(&f).unwrap();
        assert!(text.contains("[__meta__]"), "meta header; got:\n{text}");
        assert!(text.contains("schema_version = 1"), "version; got:\n{text}");
        assert!(
            text.contains("[\"aristos:foo\"]"),
            "prefixed key; got:\n{text}"
        );
        assert!(text.contains("because reasons"), "reason; got:\n{text}");
        // Absent tracking must not serialize as a key.
        assert!(
            !text.contains("tracking"),
            "no tracking key when None; got:\n{text}"
        );
    }

    #[test]
    fn accept_is_idempotent_overwrite() {
        let mut f = ExpectationsFile::default();
        f.accept(id("aristos:foo"), "first".into(), None, "t1".into());
        f.accept(id("aristos:foo"), "second".into(), None, "t2".into());
        assert_eq!(f.entries.len(), 1);
        assert_eq!(f.get(&id("aristos:foo")).unwrap().reason, "second");
    }

    #[test]
    fn malformed_file_surfaces_a_parse_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("expectations.toml");
        fs::write(&path, "this is not = = valid toml [[[").unwrap();
        assert!(ExpectationsFile::read(&path).is_err());
    }
}
