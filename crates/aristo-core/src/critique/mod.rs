//! `.aristo/critiques/<id>.critique` document schema (slice 27).
//!
//! Pure TOML; mirrors `proof::ProofFile`'s layout convention. Critique
//! findings are *advisory* — categorized prose-quality suggestions on
//! an annotation, not verification verdicts. The validator (CLI side)
//! enforces schema shape; there are no proof-tree integrity checks
//! because findings carry no derivations.
//!
//! ```toml
//! [critique]
//! critiqued_at_text_hash = "sha256:..."   # annotation text we reviewed
//! produced_at_body_hash  = "sha256:..."   # focal function body at review
//! produced_by = "aristo-critique@v0.0.7"
//! attempts = 1
//! finding_count = 2                        # derived; SDK stamps on accept
//! highest_severity = "strong-suggest"      # derived; SDK stamps on accept
//!
//! [[critique.findings]]
//! category = "rephrasing"
//! severity = "strong-suggest"
//! rationale = "Opens with double-negation; lead with the positive property."
//! suggested_text = "For every B-tree balance operation, each cell ..."
//!
//! [[critique.findings]]
//! category = "vocabulary"
//! severity = "info"
//! rationale = "Uses 'cells'; sibling annotations use 'records'."
//! # no suggested_text on info findings
//! ```
//!
//! See `aristo/docs/decisions/critique-and-pipeline-architecture.md`
//! for the design rationale (in particular the v0 enum lock for
//! `category` and `severity`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::index::Sha256;

/// One critique file: meta + a flat list of findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CritiqueFile {
    pub critique: CritiqueBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CritiqueBody {
    /// `text_hash` of the annotation at the time it was critiqued. Used
    /// for staleness anchoring — if the entry's current `text_hash`
    /// differs, the critique is stale.
    pub critiqued_at_text_hash: Sha256,
    /// `body_hash` of the focal item at the time it was critiqued.
    /// Some critique categories (e.g., `scope`) depend on the body
    /// matching the prose — body drift invalidates those findings.
    pub produced_at_body_hash: Sha256,
    pub produced_by: String,
    pub attempts: u32,
    /// Derived on accept by the SDK; the submitting agent does not need
    /// to compute this. Optional in the wire schema so an agent submitting
    /// JSON can omit it; the SDK fills it in before writing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_count: Option<u32>,
    /// Derived on accept by the SDK; the submitting agent does not need
    /// to compute this. None when `findings` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest_severity: Option<Severity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub category: Category,
    pub severity: Severity,
    pub rationale: String,
    /// Optional rewrite suggestion. Required-ish for `category = "rephrasing"`
    /// (the validator warns when missing), but truly optional for `info` /
    /// `vocabulary` / `parent-shape` findings that may just point out a
    /// pattern without proposing exact replacement text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_text: Option<String>,
}

/// Critique finding category. v0 locks five values; additions require a
/// design-doc update + validator change, lockstep. See the design doc's
/// CritiqueReport schema section for what each category means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Prose rewrite suggestion: same claim, clearer phrasing.
    Rephrasing,
    /// Issue with how this annotation relates to its parent or children
    /// (e.g., child overclaiming relative to parent; missing sibling that
    /// completes the decomposition).
    ParentShape,
    /// Word-choice inconsistency relative to siblings or project
    /// conventions (e.g., "cells" vs "records").
    Vocabulary,
    /// Claim is broader or narrower than the focal site warrants (e.g.,
    /// an invariant claim on a function that handles only one case).
    Scope,
    /// Ambiguous, vague, or weasel-worded prose.
    Clarity,
}

/// Critique finding severity. v0 locks three values; additions require a
/// design-doc update + validator change, lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Lowest severity. Informational; no action recommended.
    Info,
    /// Author should consider acting; not urgent.
    Suggest,
    /// Author should act unless they have a specific reason not to.
    StrongSuggest,
}

impl CritiqueFile {
    pub fn parse(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(label: &str) -> Sha256 {
        Sha256::from_bytes(label.as_bytes())
    }

    #[test]
    fn round_trips_through_toml() {
        let cf = CritiqueFile {
            critique: CritiqueBody {
                critiqued_at_text_hash: h("focal-text"),
                produced_at_body_hash: h("focal-body"),
                produced_by: "aristo-critique@v0.0.7".into(),
                attempts: 1,
                finding_count: Some(2),
                highest_severity: Some(Severity::StrongSuggest),
                findings: vec![
                    Finding {
                        category: Category::Rephrasing,
                        severity: Severity::StrongSuggest,
                        rationale: "double-negation".into(),
                        suggested_text: Some("For every B-tree balance ...".into()),
                    },
                    Finding {
                        category: Category::Vocabulary,
                        severity: Severity::Info,
                        rationale: "sibling drift".into(),
                        suggested_text: None,
                    },
                ],
            },
        };
        let s = cf.to_toml().unwrap();
        let back: CritiqueFile = CritiqueFile::parse(&s).unwrap();
        assert_eq!(cf, back);
    }

    #[test]
    fn category_serializes_as_kebab_case() {
        let s = toml::to_string(&Finding {
            category: Category::ParentShape,
            severity: Severity::Suggest,
            rationale: "x".into(),
            suggested_text: None,
        })
        .unwrap();
        assert!(s.contains("category = \"parent-shape\""), "got: {s}");
        assert!(s.contains("severity = \"suggest\""), "got: {s}");
    }

    #[test]
    fn severity_ordering_matches_intended_priority() {
        // Used by the SDK to derive highest_severity on accept.
        assert!(Severity::Info < Severity::Suggest);
        assert!(Severity::Suggest < Severity::StrongSuggest);
        // So the max() of a list of severities is highest_severity.
    }

    #[test]
    fn empty_findings_serializes_without_findings_field() {
        let cf = CritiqueFile {
            critique: CritiqueBody {
                critiqued_at_text_hash: h("x"),
                produced_at_body_hash: h("y"),
                produced_by: "a@v".into(),
                attempts: 1,
                finding_count: Some(0),
                highest_severity: None,
                findings: Vec::new(),
            },
        };
        let s = cf.to_toml().unwrap();
        assert!(
            !s.contains("[[critique.findings]]"),
            "empty findings must round-trip without an array; got:\n{s}"
        );
    }

    #[test]
    fn agent_may_omit_derived_fields() {
        // Agents submitting JSON do NOT need to compute finding_count or
        // highest_severity — the SDK derives them on accept. The schema
        // must accept their absence at parse time.
        let raw = r#"
[critique]
critiqued_at_text_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
produced_at_body_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
produced_by = "agent@v0"
attempts = 1

[[critique.findings]]
category = "rephrasing"
severity = "suggest"
rationale = "x"
"#;
        let cf: CritiqueFile = CritiqueFile::parse(raw).unwrap();
        assert!(cf.critique.finding_count.is_none());
        assert!(cf.critique.highest_severity.is_none());
        assert_eq!(cf.critique.findings.len(), 1);
    }
}
