//! `ProofReviewSession` — second concrete `SessionKind` impl
//! (slice 27.5, step 6).
//!
//! Proof items are referenced as `<proof_id>#<suggested_annotation_index>`
//! for inconclusive proofs' per-suggestion review, or
//! `<proof_id>#verdict` for the proof as a whole (e.g. acknowledging
//! a counterexample).
//!
//! Unlike `CritiqueReviewSession`, the per-kind callbacks here do
//! NOT mutate the `.proof` file on disk. Proofs are produced by
//! worker subagents and validated by the SDK; modifying them
//! post-validation would invalidate the chain. Instead:
//!
//! - on_accept: no-op at the file level. The user is signaling
//!   "yes, this proof's verdict is correct" or "yes, this suggested
//!   annotation should be added to source." Source mutation is a
//!   separate manual step (or a future `aristo suggestions apply`
//!   command). The session's closed-state audit trail captures
//!   the bucket.
//! - on_reject: returns a fingerprint of (proof id, suggestion
//!   text-hash if indexed, else "verdict") so future verify runs
//!   know "this suggestion was already rejected; don't surface it
//!   again."
//! - on_pending: returns a snapshot of the suggestion (or verdict
//!   metadata) for the backlog so future review can render without
//!   re-loading the proof.

use std::fs;

use aristo_core::index::AnnotationId;
use aristo_core::proof::{ProofFile, VerdictBody};

use crate::commands::verify::pending::proof_path_for;
use crate::session::kind::SessionKind;
use crate::session::types::{ItemRef, NestingPolicy};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

pub struct ProofReviewSession;

impl SessionKind for ProofReviewSession {
    fn name(&self) -> &'static str {
        "proof-review"
    }

    fn nesting_policy(&self) -> NestingPolicy {
        NestingPolicy::Disallow
    }

    fn on_accept(&self, item_ref: &ItemRef, _note: Option<&str>, ws: &Workspace) -> CliResult<()> {
        // Validate the ref points at a real proof + suggestion, so
        // the audit trail's "accepted" line is meaningful. No
        // file-level mutation — the user manually adopts source
        // changes per design.
        let _ = resolve_item(ws, item_ref)?;
        Ok(())
    }

    fn on_reject(
        &self,
        item_ref: &ItemRef,
        _note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        let resolved = resolve_item(ws, item_ref)?;
        Ok(fingerprint_for(&resolved))
    }

    fn on_pending(
        &self,
        item_ref: &ItemRef,
        _note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        let resolved = resolve_item(ws, item_ref)?;
        Ok(snapshot_for(&resolved))
    }

    fn matches_prior_rejection(
        &self,
        item_ref: &ItemRef,
        prior_fingerprint: &serde_json::Value,
    ) -> bool {
        // Best-effort: re-resolve and compare. Wraps in a workspace
        // re-discovery since the trait surface doesn't carry one.
        let Ok(ws) = Workspace::find(None) else {
            return false;
        };
        let Ok(resolved) = resolve_item(&ws, item_ref) else {
            return false;
        };
        let current = fingerprint_for(&resolved);
        current.get("proof_id") == prior_fingerprint.get("proof_id")
            && current.get("suggestion_text_hash") == prior_fingerprint.get("suggestion_text_hash")
            && current.get("kind") == prior_fingerprint.get("kind")
    }
}

// ─── helpers ─────────────────────────────────────────────────────────

/// Parsed item: which proof file, and whether it's the verdict
/// itself or a specific suggested annotation (with its text hash
/// for fingerprinting).
struct Resolved {
    proof_id: AnnotationId,
    body: ResolvedBody,
}

enum ResolvedBody {
    Verdict {
        verdict_type: &'static str,
    },
    Suggestion {
        index: usize,
        text_hash: String,
        kind_str: &'static str,
        suggested_text: String,
    },
}

fn resolve_item(ws: &Workspace, item_ref: &ItemRef) -> CliResult<Resolved> {
    let (id_str, suffix) = item_ref
        .as_str()
        .rsplit_once('#')
        .ok_or_else(|| CliError::Other {
            message: format!(
                "proof item ref `{item_ref}` is not in `<proof-id>#<index|verdict>` form"
            ),
            exit_code: 2,
        })?;
    let proof_id = AnnotationId::parse(id_str).map_err(|e| CliError::Other {
        message: format!("proof item ref carries invalid annotation id `{id_str}`: {e}"),
        exit_code: 2,
    })?;
    let path = proof_path_for(ws, &proof_id);
    let text = fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CliError::Other {
            message: format!(
                "no .proof file at {} — was this proof submitted yet?",
                path.display()
            ),
            exit_code: 2,
        },
        _ => CliError::Io(e),
    })?;
    let pf: ProofFile = toml::from_str(&text).map_err(|e| CliError::Other {
        message: format!("proof parse {}: {e}", path.display()),
        exit_code: 1,
    })?;

    let resolved_body = pf.body().map_err(|e| CliError::Other {
        message: format!("proof {proof_id} body shape: {e}"),
        exit_code: 1,
    })?;
    let body = match suffix {
        "verdict" => ResolvedBody::Verdict {
            verdict_type: verdict_type_str(&resolved_body),
        },
        other => {
            let idx: usize = other.parse().map_err(|_| CliError::Other {
                message: format!(
                    "proof item ref suffix `{other}` is not `verdict` or an integer index"
                ),
                exit_code: 2,
            })?;
            let VerdictBody::Inconclusive(inc) = &resolved_body else {
                return Err(CliError::Other {
                    message: format!(
                        "proof {proof_id} is not inconclusive; only inconclusive proofs carry indexed suggestions"
                    ),
                    exit_code: 2,
                });
            };
            let sug = inc
                .gap
                .suggested_annotations
                .get(idx)
                .ok_or_else(|| CliError::Other {
                    message: format!(
                        "proof {proof_id} has no suggested_annotations[{idx}] (has {})",
                        inc.gap.suggested_annotations.len()
                    ),
                    exit_code: 2,
                })?;
            ResolvedBody::Suggestion {
                index: idx,
                text_hash: aristo_core::hash::text_hash(&sug.suggested_text)
                    .as_str()
                    .to_string(),
                kind_str: annotation_kind_str(sug.kind),
                suggested_text: sug.suggested_text.clone(),
            }
        }
    };
    Ok(Resolved { proof_id, body })
}

fn verdict_type_str(body: &VerdictBody) -> &'static str {
    match body {
        VerdictBody::Verified(_) => "verified",
        VerdictBody::Counterexample(_) => "counterexample",
        VerdictBody::Inconclusive(_) => "inconclusive",
    }
}

fn annotation_kind_str(k: aristo_core::index::AnnotationKind) -> &'static str {
    use aristo_core::index::AnnotationKind::*;
    match k {
        Intent => "intent",
        Assume => "assume",
    }
}

fn fingerprint_for(r: &Resolved) -> serde_json::Value {
    match &r.body {
        ResolvedBody::Verdict { verdict_type } => serde_json::json!({
            "proof_id": r.proof_id.as_str(),
            "kind": "verdict",
            "verdict_type": verdict_type,
        }),
        ResolvedBody::Suggestion {
            index,
            text_hash,
            kind_str,
            ..
        } => serde_json::json!({
            "proof_id": r.proof_id.as_str(),
            "kind": "suggestion",
            "annotation_kind": kind_str,
            "index": index,
            "suggestion_text_hash": text_hash,
        }),
    }
}

fn snapshot_for(r: &Resolved) -> serde_json::Value {
    // Avoid serde_json::Value::Null in any field — the substrate's
    // backlog serializer is TOML, which has no null type. Every
    // field below is non-Option-derived so this stays safe.
    match &r.body {
        ResolvedBody::Verdict { verdict_type } => serde_json::json!({
            "proof_id": r.proof_id.as_str(),
            "kind": "verdict",
            "verdict_type": verdict_type,
        }),
        ResolvedBody::Suggestion {
            index,
            text_hash,
            kind_str,
            suggested_text,
        } => serde_json::json!({
            "proof_id": r.proof_id.as_str(),
            "kind": "suggestion",
            "annotation_kind": kind_str,
            "index": index,
            "suggestion_text_hash": text_hash,
            "suggested_text": suggested_text,
        }),
    }
}
