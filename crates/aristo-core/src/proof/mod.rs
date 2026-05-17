//! `.aristo/proofs/<id>.proof` document schema (per milestone E v2 design).
//!
//! Pure TOML — no body separator, unlike `.aristo/specs/<id>.spec`. The
//! file IS structured all the way down; nothing about a verdict needs
//! opaque text.
//!
//! ```toml
//! [verdict]
//! type = "verified"                    # | "counterexample" | "inconclusive"
//! method = "neural"
//! produced_at_text_hash = "sha256:..."
//! produced_at_body_hash = "sha256:..."
//! produced_by = "aristo-neural-verifier@v0.0.5"
//! verifier_model = "claude-sonnet-4-7"
//! attempts = 1
//! property_kind = "invariant"
//!
//! [verified.proof]
//! conclusion = "..."
//! [[verified.proof.steps]]
//! path = "0"
//! claim = "..."
//! relation_to_parent = "decomposes"
//! grounds = [ ... ]
//! ```
//!
//! The mechanical validator that gates a status flip on a verdict lives
//! in `aristo-cli`; this crate provides the types + parsing only.

use serde::{Deserialize, Serialize};

use crate::index::{AnnotationId, AnnotationKind, Sha256, VerifyMethod};

/// One `.aristo/proofs/<id>.proof` document.
///
/// The shape uses serde's adjacent-tagging via the `[verdict]` table
/// header naming which body sub-table is present. Exactly one of
/// `verified` / `counterexample` / `inconclusive` MUST be present;
/// the others MUST be absent. The post-parse [`ProofFile::body`]
/// accessor enforces this invariant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofFile {
    pub verdict: VerdictMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<VerifiedBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<CounterexampleBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inconclusive: Option<InconclusiveBody>,
}

#[aristo::intent(
    "Proof step structure is a TREE addressed by dotted paths (\"0\", \
     \"0.0\", \"0.1.2\"), not a recursive Rust struct. The flat list \
     + path-string encoding is deliberate: TOML cannot represent \
     recursive heterogeneous structures cleanly, and the path \
     addressing lets the validator, the promotion flow, and future \
     diff tooling reference any node by stable string. A refactor to \
     a recursive enum would serialize as nested arrays-of-tables that \
     are unreadable by humans and break path-based promotion \
     (`aristo promote-step --path 0.1.2`).",
    verify = "neural",
    id = "proof_tree_uses_path_addressed_flat_list"
)]
impl ProofFile {
    /// Parse from TOML text.
    pub fn parse(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    /// Serialize to canonical TOML text.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Return the body sub-table consistent with the `verdict.type`
    /// tag. Returns `Err` if zero or multiple bodies are present
    /// (the structural invariant the validator's check #1 enforces).
    pub fn body(&self) -> Result<VerdictBody<'_>, BodyShapeError> {
        let count = [
            self.verified.is_some(),
            self.counterexample.is_some(),
            self.inconclusive.is_some(),
        ]
        .iter()
        .filter(|p| **p)
        .count();
        if count != 1 {
            return Err(BodyShapeError::WrongBodyCount(count));
        }
        let body = if let Some(v) = &self.verified {
            VerdictBody::Verified(v)
        } else if let Some(c) = &self.counterexample {
            VerdictBody::Counterexample(c)
        } else {
            VerdictBody::Inconclusive(self.inconclusive.as_ref().expect("checked above"))
        };
        // Type tag must agree with which body is present.
        let agrees = matches!(
            (self.verdict.r#type, &body),
            (VerdictType::Verified, VerdictBody::Verified(_))
                | (VerdictType::Counterexample, VerdictBody::Counterexample(_))
                | (VerdictType::Inconclusive, VerdictBody::Inconclusive(_))
        );
        if !agrees {
            return Err(BodyShapeError::TagBodyMismatch {
                tag: self.verdict.r#type,
            });
        }
        Ok(body)
    }
}

/// Borrow-form view of the verdict body, returned by [`ProofFile::body`].
#[derive(Debug)]
pub enum VerdictBody<'a> {
    Verified(&'a VerifiedBody),
    Counterexample(&'a CounterexampleBody),
    Inconclusive(&'a InconclusiveBody),
}

/// Errors specific to body-shape consistency (distinct from generic
/// TOML parse errors).
#[derive(Debug, thiserror::Error)]
pub enum BodyShapeError {
    #[error("proof file has {0} body sub-tables; must have exactly 1")]
    WrongBodyCount(usize),
    #[error("verdict.type = {tag:?} but the present body sub-table doesn't match")]
    TagBodyMismatch { tag: VerdictType },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictMeta {
    /// Tag that names which body sub-table is populated. Cross-checked
    /// against actual presence in [`ProofFile::body`].
    pub r#type: VerdictType,
    /// Verify method this verdict is for. Must equal the focal entry's
    /// resolved verify level at validation time.
    pub method: VerifyMethod,
    /// Hash of the focal entry's `text` at the time the verdict was
    /// produced. Validator rejects if it differs from current.
    pub produced_at_text_hash: Sha256,
    /// Hash of the focal entry's `body` at the time the verdict was
    /// produced. Validator rejects if it differs from current.
    pub produced_at_body_hash: Sha256,
    /// Identifier of the verifier (skill/subagent name + version).
    pub produced_by: String,
    /// Underlying LLM identifier, for replay debugging. Optional —
    /// some verifiers may not know which model executed them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_model: Option<String>,
    /// How many repair attempts produced this verdict (1 = first try).
    /// Validator rejects if > K (default K=3).
    pub attempts: u32,
    /// Typed slot for downstream auto-formalization. The verifier picks
    /// one per intent; closed enum.
    pub property_kind: PropertyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerdictType {
    Verified,
    Counterexample,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PropertyKind {
    Invariant,
    Postcondition,
    Precondition,
    Equivalence,
    Safety,
    Progress,
}

// ─── verified ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedBody {
    pub proof: Proof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    /// Plain-English statement of what was proved. Should restate the
    /// focal intent's text (validator does not LLM-judge this — only
    /// structural non-emptiness).
    pub conclusion: String,
    pub steps: Vec<ProofStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofStep {
    /// Tree address. Root is `"0"`; subgoals of `"0"` are `"0.0"`,
    /// `"0.1"`, ...; subgoals of `"0.0"` are `"0.0.0"`, etc.
    pub path: String,
    pub claim: String,
    pub relation_to_parent: RelationKind,
    /// Terminal evidence for this step. At least one required (validator
    /// rejects empty grounds; see check #14).
    pub grounds: Vec<Ground>,
    /// Tree addresses of immediate subgoals. Empty for leaf nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subgoal_paths: Vec<String>,
    /// Verifier-flagged: this step's claim is reusable beyond this proof
    /// and is a candidate for promotion to a first-class intent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proposed_promotion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationKind {
    /// This step decomposes its parent's claim into subgoals.
    Decomposes,
    /// This step instantiates a universal in the parent.
    Instantiates,
    /// This step restricts the parent's domain (case split).
    Restricts,
    /// This step composes its subgoals via logical conjunction or chain.
    Composes,
    /// This step rules out a class of counterexamples to the parent.
    ExcludesCounterexample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Ground {
    Intent {
        id: AnnotationId,
        at_text_hash: Sha256,
        relation: GroundRelation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Assume {
        id: AnnotationId,
        at_text_hash: Sha256,
        relation: GroundRelation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Code {
        file: String,
        /// Line range as "<start>-<end>" or "<line>" for a single line.
        lines: String,
        code_text_hash: Sha256,
        reason: String,
    },
    PriorStep {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Composition {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroundRelation {
    Instantiates,
    Restricts,
    Composes,
    ExcludesCounterexample,
    /// Special: the cited intent was created by promoting this step's
    /// claim. Used after `aristo promote-step` rewrites a proof.
    Promoted,
}

// ─── counterexample ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterexampleBody {
    pub violation: Violation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub description: String,
    /// Path of the step in the attempted proof that the violation refutes.
    pub violated_step_path: String,
    /// Concrete trigger trace — a chain of steps demonstrating how the
    /// violating state arises. Same shape as ProofStep so the same
    /// validator can apply to trigger nodes too.
    pub trigger_steps: Vec<ProofStep>,
    /// Optional: existing intents whose claims are directly refuted by
    /// this counterexample. The validator does NOT auto-flip these to
    /// Counterexample — the user reviews and promotes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refuted_grounds: Vec<Ground>,
}

// ─── inconclusive ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InconclusiveBody {
    /// Whatever steps DID discharge before the verifier hit the gap.
    /// Optional — some inconclusive verdicts may have no partial chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_proof: Option<Proof>,
    pub gap: Gap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    pub description: String,
    /// Path of the subgoal that the verifier could not discharge.
    pub unfilled_path: String,
    /// Suggestions the user can review (queued to `.aristo/suggestions.toml`
    /// for `aristo suggestions list` in a future slice).
    pub suggested_annotations: Vec<SuggestedAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestedAnnotation {
    pub kind: AnnotationKind,
    pub suggested_text: String,
    /// Human-readable site description, e.g. `"fn redistribute (src/btree.rs:140)"`.
    pub at_site: String,
    pub rationale: String,
    /// Tree path this suggestion would close (gap.unfilled_path or one of
    /// its descendants). Optional — high-level suggestions may not pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub would_close_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::text_hash;

    fn sample_intent_id(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn sample_hash(s: &str) -> Sha256 {
        text_hash(s)
    }

    #[test]
    fn verified_proof_round_trips_through_toml() {
        let pf = ProofFile {
            verdict: VerdictMeta {
                r#type: VerdictType::Verified,
                method: VerifyMethod::Neural,
                produced_at_text_hash: sample_hash("text"),
                produced_at_body_hash: sample_hash("body"),
                produced_by: "aristo-neural-verifier@v0.0.5".to_string(),
                verifier_model: Some("claude-sonnet-4-7".to_string()),
                attempts: 1,
                property_kind: PropertyKind::Invariant,
            },
            verified: Some(VerifiedBody {
                proof: Proof {
                    conclusion: "the focal claim holds".to_string(),
                    steps: vec![ProofStep {
                        path: "0".to_string(),
                        claim: "by direct construction".to_string(),
                        relation_to_parent: RelationKind::Decomposes,
                        grounds: vec![Ground::Code {
                            file: "src/x.rs".to_string(),
                            lines: "10-20".to_string(),
                            code_text_hash: sample_hash("code"),
                            reason: "constructive proof".to_string(),
                        }],
                        subgoal_paths: vec![],
                        proposed_promotion: false,
                    }],
                },
            }),
            counterexample: None,
            inconclusive: None,
        };

        let toml = pf.to_toml().unwrap();
        let round = ProofFile::parse(&toml).unwrap();
        assert_eq!(pf, round);
    }

    #[test]
    fn body_accessor_returns_present_variant() {
        let pf = sample_verified();
        let body = pf.body().unwrap();
        assert!(matches!(body, VerdictBody::Verified(_)));
    }

    #[test]
    fn body_accessor_rejects_zero_bodies() {
        let mut pf = sample_verified();
        pf.verified = None;
        assert!(matches!(pf.body(), Err(BodyShapeError::WrongBodyCount(0))));
    }

    #[test]
    fn body_accessor_rejects_multiple_bodies() {
        let mut pf = sample_verified();
        pf.counterexample = Some(CounterexampleBody {
            violation: Violation {
                description: "x".to_string(),
                violated_step_path: "0".to_string(),
                trigger_steps: vec![],
                refuted_grounds: vec![],
            },
        });
        assert!(matches!(pf.body(), Err(BodyShapeError::WrongBodyCount(2))));
    }

    #[test]
    fn body_accessor_rejects_tag_body_mismatch() {
        let mut pf = sample_verified();
        pf.verdict.r#type = VerdictType::Counterexample; // tag says counterexample, body is verified
        assert!(matches!(
            pf.body(),
            Err(BodyShapeError::TagBodyMismatch { .. })
        ));
    }

    #[test]
    fn intent_ground_serializes_with_kind_tag() {
        let g = Ground::Intent {
            id: sample_intent_id("foo"),
            at_text_hash: sample_hash("t"),
            relation: GroundRelation::Instantiates,
            reason: Some("because".to_string()),
        };
        let s = toml::to_string(&InlineHolder { g }).unwrap();
        assert!(s.contains("kind = \"intent\""));
        assert!(s.contains("relation = \"instantiates\""));
    }

    #[test]
    fn ground_round_trips_all_variants() {
        for g in [
            Ground::Intent {
                id: sample_intent_id("foo"),
                at_text_hash: sample_hash("t"),
                relation: GroundRelation::Composes,
                reason: None,
            },
            Ground::Assume {
                id: sample_intent_id("os_zero_pages"),
                at_text_hash: sample_hash("t"),
                relation: GroundRelation::ExcludesCounterexample,
                reason: Some("rules out stale data".to_string()),
            },
            Ground::Code {
                file: "src/x.rs".to_string(),
                lines: "1-5".to_string(),
                code_text_hash: sample_hash("c"),
                reason: "see comment".to_string(),
            },
            Ground::PriorStep {
                path: "0.1".to_string(),
                reason: None,
            },
            Ground::Composition {
                reason: "subgoals combine via AND".to_string(),
            },
        ] {
            let s = toml::to_string(&InlineHolder { g: g.clone() }).unwrap();
            let r: InlineHolder = toml::from_str(&s).unwrap();
            assert_eq!(g, r.g);
        }
    }

    #[test]
    fn property_kind_serializes_kebab_case() {
        #[derive(Serialize)]
        struct Holder {
            k: PropertyKind,
        }
        let s = toml::to_string(&Holder {
            k: PropertyKind::Postcondition,
        })
        .unwrap();
        assert!(s.contains("k = \"postcondition\""), "got: {s}");
    }

    /// Helper because top-level Ground doesn't have a serde key by itself.
    #[derive(Serialize, Deserialize)]
    struct InlineHolder {
        g: Ground,
    }

    fn sample_verified() -> ProofFile {
        ProofFile {
            verdict: VerdictMeta {
                r#type: VerdictType::Verified,
                method: VerifyMethod::Neural,
                produced_at_text_hash: sample_hash("text"),
                produced_at_body_hash: sample_hash("body"),
                produced_by: "test@0".to_string(),
                verifier_model: None,
                attempts: 1,
                property_kind: PropertyKind::Invariant,
            },
            verified: Some(VerifiedBody {
                proof: Proof {
                    conclusion: "ok".to_string(),
                    steps: vec![],
                },
            }),
            counterexample: None,
            inconclusive: None,
        }
    }
}
