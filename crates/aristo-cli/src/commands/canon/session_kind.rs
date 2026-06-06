//! `IntentReviewSession` — the third concrete `SessionKind` impl
//! (§17 Slice 3).
//!
//! The `intent-review` session is the orchestrator's review surface
//! (D3/D9). Unlike `critique-review` / `proof-review`, which each
//! review one homogeneous artifact, this session carries **two item
//! types**:
//!
//! - **Primary match** (`match:<annotation-id>#<canon-id>`) — a pending
//!   match the user's own annotation produced, living in
//!   `canon-matches.toml`. Accept rewrites the annotation to canonical +
//!   binds (reuse `canon::accept`); reject fingerprints the `canon_id`.
//! - **Suggestion cluster** (`cluster:<key>`) + its **siblings**
//!   (`sibling:<key>#<canon-id>`) — the dragged-in proof-objective
//!   cluster queued by the matcher (Slice 2). The **parent is decided
//!   first** (D6): accepting the parent opens its siblings for individual
//!   decision; **rejecting the parent discards the cluster's DRAGGED-IN
//!   siblings only — any member that independently matched on its own
//!   (is/became a primary, or is already bound) is KEPT** in the normal
//!   accept/reject flow.
//!
//! ## Adoption goes through the existing write path (D4)
//!
//! Accepting a cluster parent or a sibling does **not** mutate source
//! here. Adoption funnels back through `agent writes #[aristo::intent]
//! → aristo stamp → aristo canon accept` (the skill, Slice 4). So the
//! per-kind `on_accept` for clusters/siblings is a validating no-op at
//! the file level; the session's closed-state audit trail captures the
//! decision. Only the **primary match** `on_accept` performs an
//! immediate rewrite+bind (the annotation already exists — accept
//! rewrites it).

use aristo_core::canon::cache::CanonMatchesFile;

use crate::commands::canon::{accept, suggestions};
use crate::session::kind::SessionKind;
use crate::session::rejections::{self, RejectionEntry};
use crate::session::types::{ItemRef, NestingPolicy};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// Concrete kind for the §17 intent-review loop (primary matches +
/// suggestion clusters).
pub struct IntentReviewSession;

/// Parsed shape of an `intent-review` item ref. The substrate stores
/// opaque ref strings; this kind owns the grammar.
enum IntentItem {
    /// A pending primary match: `match:<annotation-id>#<canon-id>`.
    Match {
        annotation_id: String,
        canon_id: String,
    },
    /// A cluster's parent (objective) decision: `cluster:<key>`.
    Cluster { key: String },
    /// A dragged-in sibling within a cluster: `sibling:<key>#<canon-id>`.
    Sibling { key: String, canon_id: String },
}

impl IntentItem {
    fn parse(item_ref: &ItemRef) -> CliResult<Self> {
        let s = item_ref.as_str();
        if let Some(rest) = s.strip_prefix("match:") {
            let (annotation_id, canon_id) =
                rest.rsplit_once('#').ok_or_else(|| bad_ref(item_ref))?;
            return Ok(IntentItem::Match {
                annotation_id: annotation_id.to_string(),
                canon_id: canon_id.to_string(),
            });
        }
        if let Some(rest) = s.strip_prefix("sibling:") {
            let (key, canon_id) = rest.rsplit_once('#').ok_or_else(|| bad_ref(item_ref))?;
            return Ok(IntentItem::Sibling {
                key: key.to_string(),
                canon_id: canon_id.to_string(),
            });
        }
        if let Some(key) = s.strip_prefix("cluster:") {
            return Ok(IntentItem::Cluster {
                key: key.to_string(),
            });
        }
        Err(bad_ref(item_ref))
    }
}

fn bad_ref(item_ref: &ItemRef) -> CliError {
    CliError::Other {
        message: format!(
            "intent-review item ref `{item_ref}` is not in `match:<id>#<canon-id>`, \
             `cluster:<key>`, or `sibling:<key>#<canon-id>` form"
        ),
        exit_code: 2,
    }
}

impl SessionKind for IntentReviewSession {
    fn name(&self) -> &'static str {
        suggestions::INTENT_REVIEW_KIND
    }

    fn nesting_policy(&self) -> NestingPolicy {
        NestingPolicy::Disallow
    }

    fn on_accept(&self, item_ref: &ItemRef, _note: Option<&str>, ws: &Workspace) -> CliResult<()> {
        match IntentItem::parse(item_ref)? {
            // Stage A: the user's own annotation already exists — accept
            // rewrites it to canonical + binds (reuse canon::accept).
            IntentItem::Match {
                annotation_id,
                canon_id,
            } => {
                let now = now_rfc3339();
                accept::apply_acceptance(ws, &annotation_id, &canon_id, &now)
            }
            // Stage B: adopting a cluster parent / sibling writes a NEW
            // annotation through `stamp → canon accept` (the skill, D4).
            // No file-level mutation here — validate the cluster exists
            // so the audit-trail "accepted" line is meaningful.
            IntentItem::Cluster { key } => {
                validate_cluster_exists(ws, &key)?;
                Ok(())
            }
            IntentItem::Sibling { key, canon_id } => {
                validate_sibling_exists(ws, &key, &canon_id)?;
                Ok(())
            }
        }
    }

    fn on_reject(
        &self,
        item_ref: &ItemRef,
        note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        match IntentItem::parse(item_ref)? {
            // Primary / sibling reject: fingerprint the bare canon_id so
            // dedup ②/④ suppress it on future runs.
            IntentItem::Match { canon_id, .. } => Ok(suggestions::rejection_fingerprint(&canon_id)),
            IntentItem::Sibling { canon_id, .. } => {
                Ok(suggestions::rejection_fingerprint(&canon_id))
            }
            // Parent reject: the load-bearing D6 cascade. Discard the
            // cluster's DRAGGED-IN siblings only, KEEPING any member that
            // independently matched (bound in the index, or pending /
            // accepted in the cache). Fingerprints for the discarded
            // members so they don't re-surface.
            IntentItem::Cluster { key } => {
                let obj_canon_id = discard_dragged_in_only(ws, &key, note)?;
                Ok(suggestions::rejection_fingerprint(&obj_canon_id))
            }
        }
    }

    fn on_pending(
        &self,
        item_ref: &ItemRef,
        _note: Option<&str>,
        _ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        // Backlog snapshot — enough to render the deferred item without
        // re-running the match. Avoid serde_json::Value::Null in any
        // field (the substrate's backlog serializer is TOML, which has
        // no null type).
        match IntentItem::parse(item_ref)? {
            IntentItem::Match {
                annotation_id,
                canon_id,
            } => Ok(serde_json::json!({
                "kind": "match",
                "annotation_id": annotation_id,
                "canon_id": canon_id,
            })),
            IntentItem::Cluster { key } => Ok(serde_json::json!({
                "kind": "cluster",
                "objective": key,
            })),
            IntentItem::Sibling { key, canon_id } => Ok(serde_json::json!({
                "kind": "sibling",
                "objective": key,
                "canon_id": canon_id,
            })),
        }
    }

    fn matches_prior_rejection(
        &self,
        item_ref: &ItemRef,
        prior_fingerprint: &serde_json::Value,
    ) -> bool {
        // The fingerprint is the bare canon_id; "same suggestion" ⇔ same
        // canon_id. Matches against match / sibling refs (cluster parents
        // are decided per-run, not auto-suppressed).
        let Ok(item) = IntentItem::parse(item_ref) else {
            return false;
        };
        let canon_id = match item {
            IntentItem::Match { canon_id, .. } | IntentItem::Sibling { canon_id, .. } => canon_id,
            IntentItem::Cluster { .. } => return false,
        };
        prior_fingerprint.as_str() == Some(canon_id.as_str())
    }
}

// ─── helpers ─────────────────────────────────────────────────────────

/// D6 — reject the cluster's parent. Discard the DRAGGED-IN siblings
/// only; KEEP any member that is independently held (bound in the index,
/// or pending / accepted in the cache). Returns the rejected objective's
/// canon_id (for the substrate's rejection-log fingerprint).
///
/// The discarded members are fingerprinted into the rejection log so
/// they stay suppressed; the kept members are spliced back into a
/// rewritten task (or, if every member survives, the task is left
/// intact). When nothing survives, the task file is removed entirely.
fn discard_dragged_in_only(ws: &Workspace, key: &str, note: Option<&str>) -> CliResult<String> {
    let (mut task, path) =
        suggestions::find_task_by_key(ws, key)?.ok_or_else(|| CliError::Other {
            message: format!(
                "no queued suggestion cluster `{key}` to reject.\n\
                 hint: list clusters with `aristo canon suggestions`."
            ),
            exit_code: 2,
        })?;

    let obj_canon_id = task.key().to_string();
    let cache = CanonMatchesFile::read(&ws.canon_matches_path()).map_err(CliError::Io)?;
    let now = now_rfc3339();

    let mut kept = Vec::new();
    let mut discarded = Vec::new();
    for sibling in std::mem::take(&mut task.siblings) {
        if suggestions::member_independently_held(ws, &cache, &sibling.canon_id)? {
            // Independently asserted — never discard it (D6).
            kept.push(sibling);
        } else {
            discarded.push(sibling);
        }
    }

    // Fingerprint each discarded member so it doesn't re-surface.
    for sibling in &discarded {
        rejections::append(
            ws,
            &RejectionEntry {
                ts: now.clone(),
                kind: suggestions::INTENT_REVIEW_KIND.to_string(),
                item_ref: suggestions::rejection_item_ref(&sibling.canon_id),
                note: note.map(str::to_string),
                fingerprint: suggestions::rejection_fingerprint(&sibling.canon_id),
            },
        )?;
    }

    if kept.is_empty() {
        // Nothing the user independently holds — drop the whole task.
        std::fs::remove_file(&path).map_err(CliError::Io)?;
    } else {
        // Splice the surviving (independently-held) members back and
        // rewrite the task in place. The objective is dropped — the
        // parent was rejected — so the kept members keep their own
        // accept/reject flow keyed by the seeding primary.
        task.siblings = kept;
        task.objective = None;
        let toml_text = toml::to_string_pretty(&task).map_err(|e| CliError::Other {
            message: format!("serialize suggestion task: {e}"),
            exit_code: 1,
        })?;
        std::fs::write(&path, toml_text).map_err(CliError::Io)?;
    }

    Ok(obj_canon_id)
}

/// Validate that a cluster with `key` is queued (so an accept's audit
/// line is meaningful). Returns an actionable error otherwise.
fn validate_cluster_exists(ws: &Workspace, key: &str) -> CliResult<()> {
    suggestions::find_task_by_key(ws, key)?
        .map(|_| ())
        .ok_or_else(|| CliError::Other {
            message: format!(
                "no queued suggestion cluster `{key}`.\n\
                 hint: list clusters with `aristo canon suggestions`."
            ),
            exit_code: 2,
        })
}

/// Validate that `canon_id` is a sibling of the queued cluster `key`.
fn validate_sibling_exists(ws: &Workspace, key: &str, canon_id: &str) -> CliResult<()> {
    let (task, _) = suggestions::find_task_by_key(ws, key)?.ok_or_else(|| CliError::Other {
        message: format!("no queued suggestion cluster `{key}`."),
        exit_code: 2,
    })?;
    if task.siblings.iter().any(|s| s.canon_id == canon_id) {
        Ok(())
    } else {
        Err(CliError::Other {
            message: format!("cluster `{key}` has no sibling `{canon_id}`."),
            exit_code: 2,
        })
    }
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is post-1970");
    crate::session::id_gen::format_rfc3339(now.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_match_cluster_and_sibling_refs() {
        // Match: the canon_id is the LAST `#`-segment; annotation ids may
        // themselves contain `:` (e.g. aristos:foo).
        match IntentItem::parse(&ItemRef::from_opaque("match:aristos:foo#some_canon")).unwrap() {
            IntentItem::Match {
                annotation_id,
                canon_id,
            } => {
                assert_eq!(annotation_id, "aristos:foo");
                assert_eq!(canon_id, "some_canon");
            }
            _ => panic!("expected Match"),
        }
        match IntentItem::parse(&ItemRef::from_opaque("cluster:wal_protocol")).unwrap() {
            IntentItem::Cluster { key } => assert_eq!(key, "wal_protocol"),
            _ => panic!("expected Cluster"),
        }
        match IntentItem::parse(&ItemRef::from_opaque("sibling:wal_protocol#wal_find")).unwrap() {
            IntentItem::Sibling { key, canon_id } => {
                assert_eq!(key, "wal_protocol");
                assert_eq!(canon_id, "wal_find");
            }
            _ => panic!("expected Sibling"),
        }
    }

    #[test]
    fn rejects_unprefixed_ref() {
        assert!(IntentItem::parse(&ItemRef::from_opaque("just_an_id")).is_err());
        // A `cluster:`/`match:` prefix missing the `#` is only invalid for
        // match/sibling (which need a canon_id).
        assert!(IntentItem::parse(&ItemRef::from_opaque("match:no_hash")).is_err());
        assert!(IntentItem::parse(&ItemRef::from_opaque("sibling:no_hash")).is_err());
    }

    #[test]
    fn matches_prior_rejection_compares_bare_canon_id() {
        let kind = IntentReviewSession;
        let fp = suggestions::rejection_fingerprint("wal_find");
        // Same canon_id (match or sibling) ⇒ matches.
        assert!(kind.matches_prior_rejection(&ItemRef::from_opaque("sibling:obj#wal_find"), &fp,));
        assert!(kind.matches_prior_rejection(&ItemRef::from_opaque("match:ann#wal_find"), &fp,));
        // Different canon_id ⇒ no match.
        assert!(!kind.matches_prior_rejection(&ItemRef::from_opaque("sibling:obj#other"), &fp,));
        // Cluster parents are decided per-run, never auto-suppressed.
        assert!(!kind.matches_prior_rejection(&ItemRef::from_opaque("cluster:wal_find"), &fp));
    }
}
