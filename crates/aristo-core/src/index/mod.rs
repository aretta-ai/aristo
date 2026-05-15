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

mod enums;
mod strings;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexFile {
    #[serde(rename = "__meta__")]
    pub meta: Meta,

    #[serde(flatten)]
    pub entries: BTreeMap<AnnotationId, IndexEntry>,
}

/// `[__meta__]` header carrying the schema version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// Schema version of this index file. Always `1` in the current SDK.
    pub schema_version: u32,
}

/// One annotation's record.
///
/// Server-binding fields (`linked`, `verified_outcome`, `last_verified_at_commit`)
/// are present together when the annotation is bound to the Aristo server
/// (per B5a-revised + B5b). Cross-field consistency is checked by
/// [`IndexEntry::validate`] and [`IndexFile::validate`], not by serde.
///
/// Held as a flat struct (rather than an enum over kinds or binding state) to
/// keep deserialization simple; future slices may refactor to a typed enum
/// once the second use site demands it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    /// `intent` or `assume`.
    pub kind: AnnotationKind,

    /// Natural-language annotation text.
    pub text: String,

    /// Verify spectrum value. Required for `intent`; absent for `assume`.
    /// Cross-field rule enforced by [`IndexEntry::validate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<VerifyLevel>,

    /// Current verification state.
    pub status: Status,

    /// SHA-256 hash of the canonical annotation text.
    pub text_hash: Sha256,

    /// SHA-256 hash of the covered code region's token stream (per B3).
    pub body_hash: Sha256,

    /// Source path relative to the project root.
    pub file: String,

    /// Human-readable description of the attachment site
    /// (e.g., `"fn balance_non_root"`, `"impl PageType"`).
    pub site: String,

    /// What kind of region the annotation covers.
    pub covered_region: CoveredRegion,

    /// Commit at which the current `verified_outcome` was issued.
    /// Present when the annotation has ever been verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_at_commit: Option<CommitHash>,

    /// Server-side opaque identity assigned at first bind (per B5a-revised).
    /// Present iff the annotation's id starts with `aristos:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<ArtaId>,

    /// Ed25519 verification certificate (per B5b). Present when a server-bound
    /// annotation has a non-stale verification result; absent when status is
    /// `unknown`/`stale` after binding but pre-rebind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_outcome: Option<VerifiedOutcome>,

    /// Parent linkage. Singular or list (per C1/C2 polymorphic form).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentLink>,
}

/// Parent linkage: a single id or a list of ids (AND-semantics per C1/C2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ParentLink {
    Single(AnnotationId),
    Multiple(Vec<AnnotationId>),
}

impl ParentLink {
    /// Iterate parent ids regardless of singular/list form.
    pub fn iter(&self) -> Box<dyn Iterator<Item = &AnnotationId> + '_> {
        match self {
            Self::Single(id) => Box::new(std::iter::once(id)),
            Self::Multiple(v) => Box::new(v.iter()),
        }
    }
}

/// Cross-field consistency errors detected by [`IndexEntry::validate`] and
/// [`IndexFile::validate`]. These are problems serde can't catch — they
/// involve relationships between fields rather than single-field shape.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("`assume` annotations must not carry a `verify` field")]
    AssumeHasVerify,
    #[error("`intent` annotations must carry a `verify` field")]
    IntentMissingVerify,
    #[error(
        "id `{id}` is in the `aristos:` namespace but the entry has no `linked` field; \
         server-bound entries require `linked` (B5a-revised)"
    )]
    AristosIdMissingLinked { id: String },
    #[error(
        "id `{id}` is not in the `aristos:` namespace but the entry has a `linked` field; \
         `linked` is reserved for server-bound entries (B5a-revised)"
    )]
    NonAristosIdHasLinked { id: String },
    #[error(
        "entry has `verified_outcome` but no `linked`; the outcome is meaningless without \
         the server identity it was issued for (B5b)"
    )]
    OutcomeWithoutLinked,
    #[error("`assume` annotations must not be server-bound (no `linked` / `verified_outcome`)")]
    AssumeIsServerBound,
}

impl IndexEntry {
    /// Validate intra-entry constraints that serde cannot express:
    /// `verify` ↔ `kind`, and `assume` cannot be server-bound.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self.kind {
            AnnotationKind::Intent => {
                if self.verify.is_none() {
                    return Err(ValidationError::IntentMissingVerify);
                }
            }
            AnnotationKind::Assume => {
                if self.verify.is_some() {
                    return Err(ValidationError::AssumeHasVerify);
                }
                if self.linked.is_some() || self.verified_outcome.is_some() {
                    return Err(ValidationError::AssumeIsServerBound);
                }
            }
        }
        if self.verified_outcome.is_some() && self.linked.is_none() {
            return Err(ValidationError::OutcomeWithoutLinked);
        }
        Ok(())
    }
}

impl IndexFile {
    /// Validate every entry plus cross-entry rules: namespace ↔ `linked`
    /// consistency. Returns `Ok(())` if the file is internally coherent.
    pub fn validate(&self) -> Result<(), ValidationError> {
        for (id, entry) in &self.entries {
            entry.validate()?;
            let id_is_aristos = matches!(id.namespace(), IdNamespace::Aristos);
            match (id_is_aristos, entry.linked.is_some()) {
                (true, false) => {
                    return Err(ValidationError::AristosIdMissingLinked { id: id.to_string() });
                }
                (false, true) => {
                    return Err(ValidationError::NonAristosIdHasLinked { id: id.to_string() });
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

    fn intent_local() -> IndexEntry {
        IndexEntry {
            kind: AnnotationKind::Intent,
            text: "stub".into(),
            verify: Some(VerifyLevel::Method(VerifyMethod::Test)),
            status: Status::Tested,
            text_hash: Sha256::parse(&format!("sha256:{}", "a".repeat(64))).unwrap(),
            body_hash: Sha256::parse(&format!("sha256:{}", "b".repeat(64))).unwrap(),
            file: "src/lib.rs".into(),
            site: "fn foo".into(),
            covered_region: CoveredRegion::Function,
            last_verified_at_commit: None,
            linked: None,
            verified_outcome: None,
            parent: None,
        }
    }

    fn assume_local() -> IndexEntry {
        IndexEntry {
            kind: AnnotationKind::Assume,
            verify: None,
            ..intent_local()
        }
    }

    #[test]
    fn intent_local_is_valid() {
        intent_local().validate().unwrap();
    }

    #[test]
    fn assume_local_is_valid() {
        assume_local().validate().unwrap();
    }

    #[test]
    fn assume_with_verify_rejected() {
        let mut e = assume_local();
        e.verify = Some(VerifyLevel::Method(VerifyMethod::Test));
        assert_eq!(e.validate(), Err(ValidationError::AssumeHasVerify));
    }

    #[test]
    fn intent_without_verify_rejected() {
        let mut e = intent_local();
        e.verify = None;
        assert_eq!(e.validate(), Err(ValidationError::IntentMissingVerify));
    }

    #[test]
    fn assume_with_linked_rejected() {
        let mut e = assume_local();
        e.linked = Some(ArtaId::parse("arta_op4q3z9NbV").unwrap());
        assert_eq!(e.validate(), Err(ValidationError::AssumeIsServerBound));
    }

    #[test]
    fn outcome_without_linked_rejected() {
        let mut e = intent_local();
        e.verified_outcome =
            Some(VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86))).unwrap());
        assert_eq!(e.validate(), Err(ValidationError::OutcomeWithoutLinked));
    }

    #[test]
    fn aristos_id_without_linked_rejected_at_file_level() {
        let mut entries = BTreeMap::new();
        let id = AnnotationId::parse("aristos:foo").unwrap();
        entries.insert(id, intent_local());
        let file = IndexFile {
            meta: Meta { schema_version: 1 },
            entries,
        };
        assert!(matches!(
            file.validate(),
            Err(ValidationError::AristosIdMissingLinked { .. })
        ));
    }

    #[test]
    fn non_aristos_id_with_linked_rejected_at_file_level() {
        let mut entries = BTreeMap::new();
        let id = AnnotationId::parse("foo").unwrap();
        let mut e = intent_local();
        e.linked = Some(ArtaId::parse("arta_op4q3z9NbV").unwrap());
        entries.insert(id, e);
        let file = IndexFile {
            meta: Meta { schema_version: 1 },
            entries,
        };
        assert!(matches!(
            file.validate(),
            Err(ValidationError::NonAristosIdHasLinked { .. })
        ));
    }

    #[test]
    fn parent_link_iter_handles_both_forms() {
        let single = ParentLink::Single(AnnotationId::parse("a").unwrap());
        assert_eq!(single.iter().count(), 1);

        let many = ParentLink::Multiple(vec![
            AnnotationId::parse("a").unwrap(),
            AnnotationId::parse("b").unwrap(),
            AnnotationId::parse("c").unwrap(),
        ]);
        assert_eq!(many.iter().count(), 3);
    }
}
