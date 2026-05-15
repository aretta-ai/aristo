//! [`IndexEntry`] and its sub-types: [`IntentEntry`], [`AssumeEntry`], and
//! [`BindingState`].
//!
//! ## Design: bad states unrepresentable
//!
//! `intent` and `assume` annotations have different field sets — most
//! visibly, `assume` has no `verify` (assumptions aren't verification
//! targets) and no `verified_outcome` (nothing to verify). Instead of a
//! flat struct with optional fields and a `validate()` method that
//! enforces "if kind=assume then verify must be None," we use a tagged
//! enum [`IndexEntry`] with one variant per kind. Constructing
//! `AssumeEntry { verify: Some(...) }` is a compile error rather than a
//! runtime rejection.
//!
//! Same idea for the server-binding triple. On disk, an entry can have
//! `linked`, `verified_outcome`, `last_verified_at_commit` — but only
//! three of the eight possible presence-combinations are legal:
//! - none: local-only annotation
//! - just `linked`: bound but not yet verified
//! - all three: bound and certified
//!
//! [`BindingState`] collapses these into three variants; the other five
//! combinations are unrepresentable at the API level. Wire-format
//! conversion (via [`IntentEntryWire`]) is the bottleneck where invalid
//! combinations from disk get rejected with a clear error.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    AnnotationId, ArtaId, CommitHash, CoveredRegion, Sha256, Status, VerifiedOutcome, VerifyLevel,
};

/// One annotation's record in `.aristo/index.toml`. Tagged on `kind`:
/// `kind = "intent"` deserializes as [`IntentEntry`]; `kind = "assume"`
/// as [`AssumeEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum IndexEntry {
    Intent(IntentEntry),
    Assume(AssumeEntry),
}

/// `intent` annotation: a verifiable claim about code. Carries a verify
/// level, may be server-bound, may carry a verification certificate.
///
/// Custom `Serialize` / `Deserialize` / `JsonSchema` impls flatten
/// [`BindingState`] back into the `linked` / `verified_outcome` /
/// `last_verified_at_commit` triple on disk while keeping the public API
/// typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentEntry {
    pub text: String,
    pub verify: VerifyLevel,
    pub status: Status,
    pub text_hash: Sha256,
    pub body_hash: Sha256,
    pub file: String,
    pub site: String,
    pub covered_region: CoveredRegion,
    pub binding: BindingState,
    pub parent: Option<ParentLink>,
}

/// `assume` annotation: states an external invariant the code relies on.
/// No `verify` (assumptions aren't verification targets per A5) and no
/// `verified_outcome` (nothing is being verified). MAY be server-bound:
/// the server can match an assumption against a known external-invariant
/// template (per A5 + B5a), recorded as `linked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssumeEntry {
    pub text: String,
    pub status: Status,
    pub text_hash: Sha256,
    pub body_hash: Sha256,
    pub file: String,
    pub site: String,
    pub covered_region: CoveredRegion,
    /// Server-template binding (per A5 + B5a). No certificate companion
    /// because nothing is verified — that's why this stays a single
    /// `Option` rather than a [`BindingState`]-style enum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<ArtaId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentLink>,
}

/// Server-binding state for an [`IntentEntry`]. Three legal joint values
/// of the on-disk (`linked`, `verified_outcome`, `last_verified_at_commit`)
/// triple, each one a variant; the other five combinations are
/// unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingState {
    /// Local-only annotation. No server-side identity, no certificate.
    Local,
    /// Server-bound but verification has not produced an outcome yet:
    /// after first `aristo sync`, before first `aristo verify`; or after
    /// a rename / rebind invalidated the prior outcome. Parent entry's
    /// `status` is typically [`Status::Unknown`] in this state.
    Bound { linked: ArtaId },
    /// Server-bound and verified. The certificate validates offline
    /// against the bundled SDK key registry. Parent entry's `status` may
    /// be `verified` / `tested` / `neural` / `stale` / `orphan` /
    /// `forged` / `pending-deepen` — those are diagnostic states the
    /// certificate STILL exists for, classified by the B5b four-check
    /// pipeline.
    Certified {
        linked: ArtaId,
        verified_outcome: VerifiedOutcome,
        last_verified_at_commit: CommitHash,
    },
}

impl BindingState {
    /// Construct from the on-disk field triple. Errors describe which
    /// presence-combination was invalid.
    pub fn try_from_fields(
        linked: Option<ArtaId>,
        verified_outcome: Option<VerifiedOutcome>,
        last_verified_at_commit: Option<CommitHash>,
    ) -> Result<Self, BindingFieldsError> {
        match (linked, verified_outcome, last_verified_at_commit) {
            (None, None, None) => Ok(Self::Local),
            (Some(linked), None, None) => Ok(Self::Bound { linked }),
            (Some(linked), Some(verified_outcome), Some(last_verified_at_commit)) => {
                Ok(Self::Certified {
                    linked,
                    verified_outcome,
                    last_verified_at_commit,
                })
            }
            (None, Some(_), _) | (None, _, Some(_)) => {
                Err(BindingFieldsError::OutcomeOrCommitWithoutLinked)
            }
            (Some(_), Some(_), None) => Err(BindingFieldsError::OutcomeWithoutCommit),
            (Some(_), None, Some(_)) => Err(BindingFieldsError::CommitWithoutOutcome),
        }
    }

    /// True iff this entry is server-bound (any non-`Local` variant).
    pub fn is_bound(&self) -> bool {
        !matches!(self, Self::Local)
    }
}

/// Error classifying invalid presence-combinations of the binding triple.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BindingFieldsError {
    #[error(
        "`verified_outcome` or `last_verified_at_commit` requires `linked`; \
         the outcome is meaningless without the server identity it was issued for"
    )]
    OutcomeOrCommitWithoutLinked,
    #[error("`verified_outcome` is present but `last_verified_at_commit` is missing")]
    OutcomeWithoutCommit,
    #[error("`last_verified_at_commit` is present but `verified_outcome` is missing")]
    CommitWithoutOutcome,
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

// ─── IntentEntry ↔ wire format ──────────────────────────────────────────────
//
// The on-disk shape is flat (linked / verified_outcome / last_verified_at_commit
// as siblings of text / verify / etc.); the public API is typed
// (`binding: BindingState`). `IntentEntryWire` is the bridge — derived
// `Serialize` / `Deserialize` / `JsonSchema` for the flat shape, with
// `TryFrom<IntentEntryWire> for IntentEntry` enforcing the legal-combination
// invariant at deserialize time.

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct IntentEntryWire {
    text: String,
    verify: VerifyLevel,
    status: Status,
    text_hash: Sha256,
    body_hash: Sha256,
    file: String,
    site: String,
    covered_region: CoveredRegion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    linked: Option<ArtaId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verified_outcome: Option<VerifiedOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_verified_at_commit: Option<CommitHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<ParentLink>,
}

impl Serialize for IntentEntry {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        IntentEntryWire::from(self.clone()).serialize(s)
    }
}

impl<'de> Deserialize<'de> for IntentEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = IntentEntryWire::deserialize(d)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for IntentEntry {
    fn schema_name() -> String {
        "IntentEntry".to_owned()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        // Schema describes the flat on-disk shape — that's what cross-language
        // consumers parse against. The legal-combination constraint on the
        // binding triple is captured in the type description here; codegen
        // tools that want stronger constraints can layer on `oneOf` post-hoc.
        IntentEntryWire::json_schema(generator)
    }
}

impl From<IntentEntry> for IntentEntryWire {
    fn from(e: IntentEntry) -> Self {
        let (linked, verified_outcome, last_verified_at_commit) = match e.binding {
            BindingState::Local => (None, None, None),
            BindingState::Bound { linked } => (Some(linked), None, None),
            BindingState::Certified {
                linked,
                verified_outcome,
                last_verified_at_commit,
            } => (
                Some(linked),
                Some(verified_outcome),
                Some(last_verified_at_commit),
            ),
        };
        Self {
            text: e.text,
            verify: e.verify,
            status: e.status,
            text_hash: e.text_hash,
            body_hash: e.body_hash,
            file: e.file,
            site: e.site,
            covered_region: e.covered_region,
            linked,
            verified_outcome,
            last_verified_at_commit,
            parent: e.parent,
        }
    }
}

impl TryFrom<IntentEntryWire> for IntentEntry {
    type Error = BindingFieldsError;
    fn try_from(w: IntentEntryWire) -> Result<Self, Self::Error> {
        let binding =
            BindingState::try_from_fields(w.linked, w.verified_outcome, w.last_verified_at_commit)?;
        Ok(Self {
            text: w.text,
            verify: w.verify,
            status: w.status,
            text_hash: w.text_hash,
            body_hash: w.body_hash,
            file: w.file,
            site: w.site,
            covered_region: w.covered_region,
            binding,
            parent: w.parent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AnnotationKind, VerifyMethod};
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

    fn intent_local() -> IntentEntry {
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

    fn assume_local() -> AssumeEntry {
        AssumeEntry {
            text: "external invariant".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn foo".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        }
    }

    // ─── BindingState construction ──────────────────────────────────────

    #[test]
    fn binding_local_round_trip() {
        let b = BindingState::try_from_fields(None, None, None).unwrap();
        assert_eq!(b, BindingState::Local);
        assert!(!b.is_bound());
    }

    #[test]
    fn binding_bound_round_trip() {
        let b = BindingState::try_from_fields(Some(arta()), None, None).unwrap();
        assert!(matches!(b, BindingState::Bound { .. }));
        assert!(b.is_bound());
    }

    #[test]
    fn binding_certified_round_trip() {
        let b =
            BindingState::try_from_fields(Some(arta()), Some(outcome()), Some(commit())).unwrap();
        assert!(matches!(b, BindingState::Certified { .. }));
        assert!(b.is_bound());
    }

    #[test]
    fn binding_outcome_without_linked_rejected() {
        assert_eq!(
            BindingState::try_from_fields(None, Some(outcome()), Some(commit())),
            Err(BindingFieldsError::OutcomeOrCommitWithoutLinked),
        );
    }

    #[test]
    fn binding_commit_without_linked_rejected() {
        assert_eq!(
            BindingState::try_from_fields(None, None, Some(commit())),
            Err(BindingFieldsError::OutcomeOrCommitWithoutLinked),
        );
    }

    #[test]
    fn binding_outcome_without_commit_rejected() {
        assert_eq!(
            BindingState::try_from_fields(Some(arta()), Some(outcome()), None),
            Err(BindingFieldsError::OutcomeWithoutCommit),
        );
    }

    #[test]
    fn binding_commit_without_outcome_rejected() {
        assert_eq!(
            BindingState::try_from_fields(Some(arta()), None, Some(commit())),
            Err(BindingFieldsError::CommitWithoutOutcome),
        );
    }

    // ─── IntentEntry serde round-trip ───────────────────────────────────

    #[test]
    fn intent_local_round_trips_through_json() {
        let e = intent_local();
        let json = serde_json::to_string(&e).unwrap();
        // No binding fields on disk
        assert!(!json.contains("linked"));
        assert!(!json.contains("verified_outcome"));
        let back: IntentEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn intent_certified_round_trips_through_json() {
        let mut e = intent_local();
        e.binding = BindingState::Certified {
            linked: arta(),
            verified_outcome: outcome(),
            last_verified_at_commit: commit(),
        };
        e.status = Status::Verified;
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"linked\""));
        assert!(json.contains("\"verified_outcome\""));
        assert!(json.contains("\"last_verified_at_commit\""));
        let back: IntentEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn intent_bound_round_trips_through_json() {
        let mut e = intent_local();
        e.binding = BindingState::Bound { linked: arta() };
        e.status = Status::Unknown;
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"linked\""));
        assert!(!json.contains("verified_outcome"));
        let back: IntentEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn intent_deserialize_rejects_outcome_without_linked() {
        let json = serde_json::json!({
            "text": "x",
            "verify": "test",
            "status": "verified",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
            "verified_outcome": format!("v1:{}", "A".repeat(86)),
            "last_verified_at_commit": "a".repeat(40),
        });
        let result: Result<IntentEntry, _> = serde_json::from_value(json);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires `linked`"), "got: {err}");
    }

    #[test]
    fn intent_deserialize_rejects_outcome_without_commit() {
        let json = serde_json::json!({
            "text": "x",
            "verify": "test",
            "status": "verified",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
            "linked": "arta_op4q3z9NbV",
            "verified_outcome": format!("v1:{}", "A".repeat(86)),
        });
        let result: Result<IntentEntry, _> = serde_json::from_value(json);
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("`last_verified_at_commit` is missing"),
            "got: {err}"
        );
    }

    // ─── IndexEntry tagged dispatch ─────────────────────────────────────

    #[test]
    fn indexentry_dispatches_intent_by_kind_tag() {
        let json = serde_json::json!({
            "kind": "intent",
            "text": "x",
            "verify": "test",
            "status": "tested",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
        });
        let entry: IndexEntry = serde_json::from_value(json).unwrap();
        assert!(matches!(entry, IndexEntry::Intent(_)));
    }

    #[test]
    fn indexentry_dispatches_assume_by_kind_tag() {
        let json = serde_json::json!({
            "kind": "assume",
            "text": "external",
            "status": "unknown",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
        });
        let entry: IndexEntry = serde_json::from_value(json).unwrap();
        assert!(matches!(entry, IndexEntry::Assume(_)));
    }

    #[test]
    fn assume_with_verify_field_rejected_by_serde() {
        // assume + verify is rejected by AssumeEntry's deny_unknown_fields
        // (verify isn't a field of AssumeEntry).
        let json = serde_json::json!({
            "kind": "assume",
            "text": "external",
            "verify": "test",
            "status": "unknown",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
        });
        let result: Result<IndexEntry, _> = serde_json::from_value(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("verify"), "got: {err}");
    }

    #[test]
    fn assume_with_verified_outcome_rejected_by_serde() {
        let json = serde_json::json!({
            "kind": "assume",
            "text": "external",
            "status": "unknown",
            "text_hash": format!("sha256:{}", "a".repeat(64)),
            "body_hash": format!("sha256:{}", "b".repeat(64)),
            "file": "src/lib.rs",
            "site": "fn foo",
            "covered_region": "function",
            "verified_outcome": format!("v1:{}", "A".repeat(86)),
        });
        let result: Result<IndexEntry, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn assume_can_be_server_bound() {
        // Per A5: assumes can be server-bound (linked) but not verified.
        let mut a = assume_local();
        a.linked = Some(arta());
        let json = serde_json::to_string(&IndexEntry::Assume(a.clone())).unwrap();
        let back: IndexEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, IndexEntry::Assume(a));
    }

    // ─── ParentLink ─────────────────────────────────────────────────────

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

    #[test]
    fn parent_link_serializes_singular_as_string() {
        let single = ParentLink::Single(AnnotationId::parse("foo").unwrap());
        assert_eq!(serde_json::to_string(&single).unwrap(), "\"foo\"");
    }

    #[test]
    fn parent_link_serializes_multiple_as_array() {
        let many = ParentLink::Multiple(vec![
            AnnotationId::parse("a").unwrap(),
            AnnotationId::parse("b").unwrap(),
        ]);
        assert_eq!(serde_json::to_string(&many).unwrap(), "[\"a\",\"b\"]");
    }

    // ─── Tagged-enum kind discrimination via AnnotationKind ─────────────

    #[test]
    fn kind_discriminator_matches_annotation_kind_serialization() {
        // AnnotationKind serializes as lowercase; IndexEntry's tag uses the
        // same form. Stays in sync because both use rename_all="lowercase".
        assert_eq!(
            serde_json::to_value(AnnotationKind::Intent).unwrap(),
            serde_json::json!("intent")
        );
        assert_eq!(
            serde_json::to_value(AnnotationKind::Assume).unwrap(),
            serde_json::json!("assume")
        );
    }
}
