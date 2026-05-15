//! `.aristo/index.toml` document schema (per D1 + B5a-revised + B5b).
//!
//! The on-disk shape is a TOML document with a `[__meta__]` header carrying
//! `schema_version` and one top-level table per annotation, keyed by id.
//! Server-bound entries use the quoted-key form `["aristos:<name>"]` because
//! their id contains a colon.
//!
//! ```toml
//! [__meta__]
//! schema_version = 1
//!
//! ["aristos:balance_no_duplicate_cells"]
//! kind   = "intent"
//! text   = "..."
//! verify = "full"
//! status = "verified"
//! linked = "arta_op4q3z9NbV"
//! verified_outcome = "v1:p7XnKqL9..."
//! ...
//! ```

mod entry;
mod enums;
mod strings;

pub use entry::{
    AssumeEntry, BindingFieldsError, BindingState, IndexEntry, IntentEntry, ParentLink,
};
pub use enums::{AnnotationKind, CoveredRegion, Status, VerifyLevel, VerifyMethod};
pub use strings::{
    AnnotationId, ArtaId, CommitHash, IdNamespace, ParseError, Sha256, VerifiedOutcome,
};

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level `.aristo/index.toml` document.
///
/// `__meta__` carries the schema version (currently `1`); every other key is
/// an annotation id mapping to its [`IndexEntry`]. Iteration order follows
/// `BTreeMap`'s sorted-by-key semantics so on-disk ordering is deterministic.
///
/// Note: serde forbids `#[serde(deny_unknown_fields)]` together with
/// `#[serde(flatten)]`, so unknown top-level keys flow into `entries`
/// and either parse as `IndexEntry` (legal addition) or fail to parse
/// against the entry schema (clear "unknown variant" / "missing field"
/// error). Unknown-field rejection still happens INSIDE each entry via
/// `IntentEntryWire` / `AssumeEntry`'s own `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IndexFile {
    #[serde(rename = "__meta__")]
    pub meta: Meta,

    #[serde(flatten)]
    pub entries: BTreeMap<AnnotationId, IndexEntry>,
}

/// `[__meta__]` header carrying the schema version plus optional generator
/// metadata (who/when/where wrote the file; useful for debugging stale
/// indices but not load-bearing for any logic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// Schema version of this index file. Always `1` in the current SDK.
    pub schema_version: u32,

    /// Identity of the writer (e.g., `"aristo index v0.1.0"`).
    /// Optional; informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,

    /// Timestamp of last write, RFC 3339 format
    /// (e.g., `"2026-05-13T14:23:00Z"`). Optional; informational only.
    /// String-typed (not a `DateTime`) to avoid pulling chrono / toml::Datetime
    /// into the public API; consumers can parse if needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,

    /// Path to the source root the index was generated from, relative to
    /// the project root (e.g., `"."`). Optional; informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_root: Option<String>,
}

/// Cross-entry validation error: an annotation's id namespace must agree
/// with its binding state. Within-entry shape (kind ↔ field set, binding-
/// triple legality) is enforced by the type system + serde — see the
/// [`entry`] module.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error(
        "id `{id}` is in the `aristos:` namespace but the entry is not server-bound; \
         server-bound entries require a `linked` field (B5a-revised)"
    )]
    AristosIdNotBound { id: String },
    #[error(
        "id `{id}` is not in the `aristos:` namespace but the entry has a `linked` field; \
         the namespace prefix and the binding must agree (B5a-revised)"
    )]
    NonAristosIdIsBound { id: String },
}

/// Produce the canonical JSON Schema (draft 2020-12 via schemars 0.8)
/// describing `.aristo/index.toml`. Output is the source of truth for
/// non-Rust language SDKs that need to validate index files without
/// re-implementing the constraint logic.
///
/// The `dump-schemas` example writes this string to
/// `schemas/aristo-index.schema.json` at the workspace root; the
/// `tests/schemas.rs` integration test re-generates it and diffs
/// against the committed file as a CI gate.
pub fn index_file_schema_json() -> String {
    let schema = schemars::schema_for!(IndexFile);
    serde_json::to_string_pretty(&schema)
        .expect("serializing a schemars-derived schema cannot fail")
}

impl IndexFile {
    /// Validate every entry's id-namespace ↔ binding-state agreement.
    pub fn validate(&self) -> Result<(), ValidationError> {
        for (id, entry) in &self.entries {
            let id_is_aristos = matches!(id.namespace(), IdNamespace::Aristos);
            let entry_is_bound = match entry {
                IndexEntry::Intent(e) => e.binding.is_bound(),
                IndexEntry::Assume(e) => e.linked.is_some(),
            };
            match (id_is_aristos, entry_is_bound) {
                (true, false) => {
                    return Err(ValidationError::AristosIdNotBound { id: id.to_string() });
                }
                (false, true) => {
                    return Err(ValidationError::NonAristosIdIsBound { id: id.to_string() });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(byte: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", byte.to_string().repeat(64))).unwrap()
    }
    fn arta() -> ArtaId {
        ArtaId::parse("arta_op4q3z9NbV").unwrap()
    }
    fn outcome() -> VerifiedOutcome {
        VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86))).unwrap()
    }
    fn commit() -> CommitHash {
        CommitHash::parse(&"a".repeat(40)).unwrap()
    }

    fn local_intent() -> IntentEntry {
        IntentEntry {
            text: "stub".into(),
            verify: VerifyLevel::Method(VerifyMethod::Test),
            status: Status::Tested,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
        }
    }

    fn certified_intent() -> IntentEntry {
        IntentEntry {
            binding: BindingState::Certified {
                linked: arta(),
                verified_outcome: outcome(),
                last_verified_at_commit: commit(),
            },
            status: Status::Verified,
            ..local_intent()
        }
    }

    fn local_assume() -> AssumeEntry {
        AssumeEntry {
            text: "external".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "mod storage".into(),
            covered_region: CoveredRegion::ModuleInlineBody,
            linked: None,
            parent: None,
        }
    }

    fn meta_minimal() -> Meta {
        Meta {
            schema_version: 1,
            generated_by: None,
            generated_at: None,
            source_root: None,
        }
    }

    #[test]
    fn empty_index_validates() {
        let file = IndexFile {
            meta: meta_minimal(),
            entries: BTreeMap::new(),
        };
        file.validate().unwrap();
    }

    #[test]
    fn meta_optional_fields_default_to_none() {
        let json = serde_json::json!({ "schema_version": 1 });
        let meta: Meta = serde_json::from_value(json).unwrap();
        assert_eq!(meta.schema_version, 1);
        assert!(meta.generated_by.is_none());
        assert!(meta.generated_at.is_none());
        assert!(meta.source_root.is_none());
    }

    #[test]
    fn meta_optional_fields_round_trip() {
        let meta = Meta {
            schema_version: 1,
            generated_by: Some("aristo index v0.1.0".into()),
            generated_at: Some("2026-05-13T14:23:00Z".into()),
            source_root: Some(".".into()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: Meta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn meta_rejects_unknown_field() {
        let json = serde_json::json!({ "schema_version": 1, "bogus": 42 });
        let result: Result<Meta, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn aristos_id_with_certified_intent_validates() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("aristos:foo").unwrap(),
            IndexEntry::Intent(certified_intent()),
        );
        let file = IndexFile {
            meta: meta_minimal(),
            entries,
        };
        file.validate().unwrap();
    }

    #[test]
    fn aristos_id_with_local_intent_rejected() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("aristos:foo").unwrap(),
            IndexEntry::Intent(local_intent()),
        );
        let file = IndexFile {
            meta: meta_minimal(),
            entries,
        };
        assert!(matches!(
            file.validate(),
            Err(ValidationError::AristosIdNotBound { .. })
        ));
    }

    #[test]
    fn non_aristos_id_with_certified_intent_rejected() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("foo").unwrap(),
            IndexEntry::Intent(certified_intent()),
        );
        let file = IndexFile {
            meta: meta_minimal(),
            entries,
        };
        assert!(matches!(
            file.validate(),
            Err(ValidationError::NonAristosIdIsBound { .. })
        ));
    }

    #[test]
    fn aristos_id_with_bound_assume_validates() {
        // Per A5: assumes can be server-bound (linked only, no certificate).
        let mut a = local_assume();
        a.linked = Some(arta());
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("aristos:atomic_writes").unwrap(),
            IndexEntry::Assume(a),
        );
        let file = IndexFile {
            meta: meta_minimal(),
            entries,
        };
        file.validate().unwrap();
    }

    #[test]
    fn aristos_id_with_local_assume_rejected() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AnnotationId::parse("aristos:atomic_writes").unwrap(),
            IndexEntry::Assume(local_assume()),
        );
        let file = IndexFile {
            meta: meta_minimal(),
            entries,
        };
        assert!(matches!(
            file.validate(),
            Err(ValidationError::AristosIdNotBound { .. })
        ));
    }
}
