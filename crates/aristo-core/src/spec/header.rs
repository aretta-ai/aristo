//! [`SpecHeader`] for `.aristo/specs/<id>.spec` files.
//!
//! ## Design: bad states unrepresentable
//!
//! The on-disk header has two fields that signal staleness — `stale_at`
//! and `current_body_hash` — and they are only legal jointly: either
//! both absent (the spec is fresh) or both present (the spec is stale,
//! detected at the timestamp, against the current body hash). The
//! [`StalenessState`] enum collapses these into two variants; the two
//! invalid combinations (one without the other) are unrepresentable
//! at the API level. Wire-format conversion via [`SpecHeaderWire`] is
//! the bottleneck where on-disk fields get checked once and converted
//! to the typed shape.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::index::{AnnotationId, CoveredRegion, Sha256};

/// TOML header of a `.aristo/specs/<id>.spec` file.
///
/// Records mined-assertion metadata (per G5 + B3): identifies the
/// annotation, captures the body and text hashes at mining time,
/// marks the covered region, and timestamps both the mining itself
/// and any subsequent staleness detection by `aristo stamp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecHeader {
    /// Id of the annotation this spec was mined for.
    pub annotation_id: AnnotationId,
    /// Hash of the annotation text at the time the assertion was mined.
    pub annotation_text_hash: Sha256,
    /// Hash of the covered code region's token stream at mining time.
    pub source_body_hash: Sha256,
    /// What kind of region the assertion covers (per B3 covered-region rules).
    pub covered_region: CoveredRegion,
    /// Human-readable site path, e.g. `"src/btree.rs::insert_into_cell"`.
    pub covered_region_path: String,
    /// Timestamp of the mining run (RFC 3339 string).
    pub mined_at: String,
    /// Identity of the miner (skill name, host agent, model class).
    pub mined_by: String,
    /// True if a human has reviewed and approved the mined assertion.
    /// `aristo verify` will not silently overwrite a `human_reviewed`
    /// spec on staleness — it produces a `.candidate` file instead.
    pub human_reviewed: bool,
    /// Free-text notes (may be empty).
    pub notes: String,
    /// Staleness state. `Fresh` means current; `Stale` means
    /// `aristo stamp` detected drift from the source body.
    pub staleness: StalenessState,
}

/// Whether a spec is current relative to its annotated source body.
///
/// Two on-disk fields (`stale_at`, `current_body_hash`) only have two
/// legal joint presence-states: both absent (`Fresh`) or both present
/// (`Stale`). The two illegal mixes are unrepresentable here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessState {
    /// Spec is in sync with the source body — nothing to do.
    Fresh,
    /// `aristo stamp` detected that the source body's token-stream hash
    /// no longer matches `source_body_hash`. The next `aristo verify`
    /// re-mines (unless `human_reviewed` is true, in which case it
    /// produces a `.candidate` file for human review).
    Stale {
        /// Timestamp at which staleness was detected (RFC 3339 string).
        detected_at: String,
        /// Body hash computed by `aristo stamp` at detection time.
        current_body_hash: Sha256,
    },
}

impl StalenessState {
    /// Construct from the on-disk field pair. Errors describe which
    /// invalid combination was supplied.
    pub fn try_from_fields(
        stale_at: Option<String>,
        current_body_hash: Option<Sha256>,
    ) -> Result<Self, StalenessFieldsError> {
        match (stale_at, current_body_hash) {
            (None, None) => Ok(Self::Fresh),
            (Some(detected_at), Some(current_body_hash)) => Ok(Self::Stale {
                detected_at,
                current_body_hash,
            }),
            (Some(_), None) => Err(StalenessFieldsError::StaleAtWithoutCurrentBodyHash),
            (None, Some(_)) => Err(StalenessFieldsError::CurrentBodyHashWithoutStaleAt),
        }
    }

    /// True iff this spec has been flagged stale.
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }
}

/// Error classifying invalid presence-combinations of the staleness pair.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StalenessFieldsError {
    #[error("`stale_at` is present but `current_body_hash` is missing")]
    StaleAtWithoutCurrentBodyHash,
    #[error("`current_body_hash` is present but `stale_at` is missing")]
    CurrentBodyHashWithoutStaleAt,
}

// ─── SpecHeader ↔ wire format ───────────────────────────────────────────────
//
// On disk, the staleness fields are flat siblings of the always-present
// header fields. The Wire struct has them as Options; conversion via
// TryFrom enforces "both or neither."

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SpecHeaderWire {
    annotation_id: AnnotationId,
    annotation_text_hash: Sha256,
    source_body_hash: Sha256,
    covered_region: CoveredRegion,
    covered_region_path: String,
    mined_at: String,
    mined_by: String,
    human_reviewed: bool,
    #[serde(default)]
    notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stale_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_body_hash: Option<Sha256>,
}

impl Serialize for SpecHeader {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        SpecHeaderWire::from(self.clone()).serialize(s)
    }
}

impl<'de> Deserialize<'de> for SpecHeader {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = SpecHeaderWire::deserialize(d)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for SpecHeader {
    fn schema_name() -> String {
        "SpecHeader".to_owned()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        SpecHeaderWire::json_schema(generator)
    }
}

impl From<SpecHeader> for SpecHeaderWire {
    fn from(h: SpecHeader) -> Self {
        let (stale_at, current_body_hash) = match h.staleness {
            StalenessState::Fresh => (None, None),
            StalenessState::Stale {
                detected_at,
                current_body_hash,
            } => (Some(detected_at), Some(current_body_hash)),
        };
        Self {
            annotation_id: h.annotation_id,
            annotation_text_hash: h.annotation_text_hash,
            source_body_hash: h.source_body_hash,
            covered_region: h.covered_region,
            covered_region_path: h.covered_region_path,
            mined_at: h.mined_at,
            mined_by: h.mined_by,
            human_reviewed: h.human_reviewed,
            notes: h.notes,
            stale_at,
            current_body_hash,
        }
    }
}

impl TryFrom<SpecHeaderWire> for SpecHeader {
    type Error = StalenessFieldsError;
    fn try_from(w: SpecHeaderWire) -> Result<Self, Self::Error> {
        let staleness = StalenessState::try_from_fields(w.stale_at, w.current_body_hash)?;
        Ok(Self {
            annotation_id: w.annotation_id,
            annotation_text_hash: w.annotation_text_hash,
            source_body_hash: w.source_body_hash,
            covered_region: w.covered_region,
            covered_region_path: w.covered_region_path,
            mined_at: w.mined_at,
            mined_by: w.mined_by,
            human_reviewed: w.human_reviewed,
            notes: w.notes,
            staleness,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn fresh_header() -> SpecHeader {
        SpecHeader {
            annotation_id: AnnotationId::parse("insert_into_cell_postcondition").unwrap(),
            annotation_text_hash: sha('a'),
            source_body_hash: sha('b'),
            covered_region: CoveredRegion::Function,
            covered_region_path: "src/btree.rs::insert_into_cell".into(),
            mined_at: "2026-05-13T14:23:00Z".into(),
            mined_by: "aristo verify (skill=aristo-mine-assertions)".into(),
            human_reviewed: false,
            notes: String::new(),
            staleness: StalenessState::Fresh,
        }
    }

    // ─── StalenessState construction ────────────────────────────────────

    #[test]
    fn staleness_fresh_round_trip() {
        let s = StalenessState::try_from_fields(None, None).unwrap();
        assert_eq!(s, StalenessState::Fresh);
        assert!(!s.is_stale());
    }

    #[test]
    fn staleness_stale_round_trip() {
        let s =
            StalenessState::try_from_fields(Some("2026-05-15T09:14:22Z".into()), Some(sha('c')))
                .unwrap();
        assert!(matches!(s, StalenessState::Stale { .. }));
        assert!(s.is_stale());
    }

    #[test]
    fn staleness_stale_at_alone_rejected() {
        assert_eq!(
            StalenessState::try_from_fields(Some("ts".into()), None),
            Err(StalenessFieldsError::StaleAtWithoutCurrentBodyHash),
        );
    }

    #[test]
    fn staleness_current_hash_alone_rejected() {
        assert_eq!(
            StalenessState::try_from_fields(None, Some(sha('c'))),
            Err(StalenessFieldsError::CurrentBodyHashWithoutStaleAt),
        );
    }

    // ─── SpecHeader serde round-trip ────────────────────────────────────

    #[test]
    fn fresh_header_round_trips_through_toml() {
        let h = fresh_header();
        let toml_text = toml::to_string(&h).unwrap();
        // Stale fields absent on disk
        assert!(!toml_text.contains("stale_at"));
        assert!(!toml_text.contains("current_body_hash"));
        let back: SpecHeader = toml::from_str(&toml_text).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn stale_header_round_trips_through_toml() {
        let mut h = fresh_header();
        h.staleness = StalenessState::Stale {
            detected_at: "2026-05-15T09:14:22Z".into(),
            current_body_hash: sha('c'),
        };
        let toml_text = toml::to_string(&h).unwrap();
        assert!(toml_text.contains("stale_at"));
        assert!(toml_text.contains("current_body_hash"));
        let back: SpecHeader = toml::from_str(&toml_text).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn header_deserialize_rejects_stale_at_alone() {
        let toml_text = format!(
            r#"
annotation_id = "x"
annotation_text_hash = "sha256:{a}"
source_body_hash = "sha256:{b}"
covered_region = "function"
covered_region_path = "src/x.rs::y"
mined_at = "2026-05-13T14:23:00Z"
mined_by = "aristo verify"
human_reviewed = false
notes = ""
stale_at = "2026-05-15T09:14:22Z"
"#,
            a = "a".repeat(64),
            b = "b".repeat(64)
        );
        let result: Result<SpecHeader, _> = toml::from_str(&toml_text);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("stale_at"), "got: {err}");
    }

    #[test]
    fn header_deserialize_rejects_unknown_field() {
        let toml_text = format!(
            r#"
annotation_id = "x"
annotation_text_hash = "sha256:{a}"
source_body_hash = "sha256:{b}"
covered_region = "function"
covered_region_path = "src/x.rs::y"
mined_at = "2026-05-13T14:23:00Z"
mined_by = "aristo verify"
human_reviewed = false
notes = ""
this_is_not_a_field = "rejected"
"#,
            a = "a".repeat(64),
            b = "b".repeat(64)
        );
        let result: Result<SpecHeader, _> = toml::from_str(&toml_text);
        assert!(result.is_err());
    }

    #[test]
    fn human_reviewed_field_round_trips_both_values() {
        let mut h = fresh_header();
        for value in [true, false] {
            h.human_reviewed = value;
            let toml_text = toml::to_string(&h).unwrap();
            let back: SpecHeader = toml::from_str(&toml_text).unwrap();
            assert_eq!(back.human_reviewed, value);
        }
    }
}
