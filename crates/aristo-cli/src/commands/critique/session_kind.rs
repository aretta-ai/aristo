//! `CritiqueReviewSession` — first concrete `SessionKind` impl
//! (slice 27.5, step 5).
//!
//! Critique items are referenced as `<critique_id>#<finding_index>`
//! (e.g. `critique_queue_entries_are_self_contained#2`). Each
//! disposition decision mutates the corresponding finding inside
//! `.aristo/critiques/<critique_id>.critique`:
//!
//! - accept → `findings[index].disposition = "accepted"`.
//! - reject → `findings[index].disposition = "rejected"` plus a
//!   fingerprint of (category, normalized rationale prefix) so future
//!   critique runs can recognize "same suggestion".
//! - pending → `findings[index].disposition = "deferred"` plus a
//!   backlog data payload snapshotting the finding body so a future
//!   review can render it without re-running the critique worker.
//!
//! Text-drift invalidation: if the focal annotation's text changes,
//! the next critique run re-generates the `.critique` file from
//! scratch (apply step rejects on text-hash mismatch). Disposition
//! history is intentionally lost — the prior review was about prior
//! text. See `docs/decisions/review-sessions.md` and the older
//! `critique-finding-disposition.md` for the design rationale.

use std::fs;

use aristo_core::critique::{CritiqueFile, Disposition, Finding};
use aristo_core::index::AnnotationId;

use crate::commands::critique::pending::critique_path_for;
use crate::commands::index::atomic_write;
use crate::session::kind::SessionKind;
use crate::session::types::{ItemRef, NestingPolicy};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// Concrete kind for prose-critique findings.
pub struct CritiqueReviewSession;

impl SessionKind for CritiqueReviewSession {
    fn name(&self) -> &'static str {
        "critique-review"
    }

    fn nesting_policy(&self) -> NestingPolicy {
        NestingPolicy::Disallow
    }

    #[aristo::intent(
        "Accepting a critique finding stamps `disposition = accepted` \
         into the corresponding finding inside `.aristo/critiques/\
         <id>.critique`. Future `aristo critique --apply-findings` \
         runs hide closed findings by default (the loop is closed: \
         a reviewed finding doesn't re-surface). A refactor that \
         updated the substrate's session state without touching the \
         .critique file would leave the finding visible to every \
         subsequent apply run as if no review had happened.",
        verify = "neural",
        id = "critique_on_accept_stamps_disposition_in_critique_file"
    )]
    fn on_accept(&self, item_ref: &ItemRef, note: Option<&str>, ws: &Workspace) -> CliResult<()> {
        stamp_disposition(ws, item_ref, Disposition::Accepted, note)
    }

    fn on_reject(
        &self,
        item_ref: &ItemRef,
        note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        let fingerprint = with_finding(ws, item_ref, |f| Ok(make_fingerprint(f)))?;
        stamp_disposition(ws, item_ref, Disposition::Rejected, note)?;
        Ok(fingerprint)
    }

    fn on_pending(
        &self,
        item_ref: &ItemRef,
        note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value> {
        let snapshot = with_finding(ws, item_ref, |f| Ok(snapshot_for_backlog(f)))?;
        stamp_disposition(ws, item_ref, Disposition::Deferred, note)?;
        Ok(snapshot)
    }

    fn matches_prior_rejection(
        &self,
        item_ref: &ItemRef,
        prior_fingerprint: &serde_json::Value,
    ) -> bool {
        // We can't fully compute the new item's fingerprint without
        // loading its current .critique file. The skill body that
        // calls this (step 7) will have the candidate finding in
        // hand and can compute a fingerprint then call this with
        // both — but our trait surface here works against item_ref +
        // prior. Compromise: load the finding from disk on demand.
        let Ok(current) = with_finding_value(item_ref, |_ws_dummy, f| make_fingerprint(f)) else {
            return false;
        };
        fingerprints_match(&current, prior_fingerprint)
    }
}

// ─── helpers ─────────────────────────────────────────────────────────

/// Parse `<id>#<index>` ref into (annotation id, finding index).
/// Returns an error mapping that surfaces a useful refusal to the
/// user, not a panic — refs come from the CLI / skill, not from
/// substrate-internal generation, so user typos are common.
fn parse_ref(item_ref: &ItemRef) -> CliResult<(AnnotationId, usize)> {
    let (id_str, idx) = item_ref.split_indexed().ok_or_else(|| CliError::Other {
        message: format!(
            "critique item ref `{}` is not in `<critique-id>#<finding-index>` form",
            item_ref
        ),
        exit_code: 2,
    })?;
    let id = AnnotationId::parse(id_str).map_err(|e| CliError::Other {
        message: format!("critique item ref carries invalid annotation id `{id_str}`: {e}"),
        exit_code: 2,
    })?;
    Ok((id, idx))
}

fn load_critique(
    ws: &Workspace,
    id: &AnnotationId,
) -> CliResult<(CritiqueFile, std::path::PathBuf)> {
    let path = critique_path_for(ws, id);
    let text = fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CliError::Other {
            message: format!(
                "no .critique file at {} — was this critique submitted yet?",
                path.display()
            ),
            exit_code: 2,
        },
        _ => CliError::Io(e),
    })?;
    let cf = CritiqueFile::parse(&text).map_err(|e| CliError::Other {
        message: format!("critique parse {}: {e}", path.display()),
        exit_code: 1,
    })?;
    Ok((cf, path))
}

fn with_finding<F, T>(ws: &Workspace, item_ref: &ItemRef, f: F) -> CliResult<T>
where
    F: FnOnce(&Finding) -> CliResult<T>,
{
    let (id, idx) = parse_ref(item_ref)?;
    let (cf, path) = load_critique(ws, &id)?;
    let finding = cf
        .critique
        .findings
        .get(idx)
        .ok_or_else(|| CliError::Other {
            message: format!(
                "critique {} has no finding at index {idx} (has {} findings)",
                path.display(),
                cf.critique.findings.len()
            ),
            exit_code: 2,
        })?;
    f(finding)
}

/// Reader-side variant used by matches_prior_rejection (which is
/// invoked outside the on_* call chain that already has a workspace
/// handle). The workspace gets re-discovered from cwd inside.
#[allow(
    dead_code,
    reason = "consumed by matches_prior_rejection in step 7+ skill seeding"
)]
fn with_finding_value<F, T>(item_ref: &ItemRef, f: F) -> CliResult<T>
where
    F: FnOnce(&Workspace, &Finding) -> T,
{
    let ws = Workspace::find(None).map_err(|e| match e {
        crate::workspace::WorkspaceError::NotFound { searched_from } => {
            CliError::NotInWorkspace { searched_from }
        }
    })?;
    let (id, idx) = parse_ref(item_ref)?;
    let (cf, _path) = load_critique(&ws, &id)?;
    let finding = cf
        .critique
        .findings
        .get(idx)
        .ok_or_else(|| CliError::Other {
            message: format!("critique {id} has no finding at index {idx}"),
            exit_code: 2,
        })?;
    Ok(f(&ws, finding))
}

#[aristo::intent(
    "Stamping a disposition mutates the on-disk .critique file via \
     atomic_write (temp + rename). The .critique file is the audit \
     trail of every triage decision against this critique; a \
     non-atomic write would deserialize-fail on partial state and \
     look to `--apply-findings` like the critique has vanished.",
    verify = "neural",
    id = "stamp_disposition_uses_atomic_write"
)]
fn stamp_disposition(
    ws: &Workspace,
    item_ref: &ItemRef,
    disposition: Disposition,
    note: Option<&str>,
) -> CliResult<()> {
    let (id, idx) = parse_ref(item_ref)?;
    let (mut cf, path) = load_critique(ws, &id)?;
    let finding = cf
        .critique
        .findings
        .get_mut(idx)
        .ok_or_else(|| CliError::Other {
            message: format!("critique {} has no finding at index {idx}", path.display()),
            exit_code: 2,
        })?;
    finding.disposition = Some(disposition);
    finding.disposition_note = note.map(|s| s.to_string());
    finding.closed_at = Some(now_rfc3339());
    let serialized = cf.to_toml().map_err(|e| CliError::Other {
        message: format!("critique serialize: {e}"),
        exit_code: 1,
    })?;
    atomic_write(&path, &serialized)
}

/// Fingerprint for `matches_prior_rejection`: per-kind heuristic for
/// "this is the same finding the user already rejected." For critique
/// we use the (category, first 100 chars of rationale) tuple — same
/// category + similar rationale prefix ≈ same finding. Refinement is
/// parked as open question Q1 in the design doc.
fn make_fingerprint(f: &Finding) -> serde_json::Value {
    let rationale_sketch: String = f.rationale.chars().take(100).collect();
    serde_json::json!({
        "category": category_str(f.category),
        "rationale_sketch": rationale_sketch.trim(),
    })
}

#[allow(
    dead_code,
    reason = "consumed by matches_prior_rejection in step 7+ skill seeding"
)]
fn fingerprints_match(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    a.get("category") == b.get("category") && a.get("rationale_sketch") == b.get("rationale_sketch")
}

fn snapshot_for_backlog(f: &Finding) -> serde_json::Value {
    serde_json::json!({
        "category": category_str(f.category),
        "severity": severity_str(f.severity),
        "rationale": f.rationale,
        "suggested_text": f.suggested_text,
    })
}

fn category_str(c: aristo_core::critique::Category) -> &'static str {
    use aristo_core::critique::Category::*;
    match c {
        Rephrasing => "rephrasing",
        ParentShape => "parent-shape",
        Vocabulary => "vocabulary",
        Scope => "scope",
        Clarity => "clarity",
    }
}

fn severity_str(s: aristo_core::critique::Severity) -> &'static str {
    use aristo_core::critique::Severity::*;
    match s {
        Info => "info",
        Suggest => "suggest",
        StrongSuggest => "strong-suggest",
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
    use aristo_core::critique::{Category, CritiqueBody, Severity};
    use aristo_core::index::Sha256;
    use tempfile::TempDir;

    fn fresh_workspace(dir: &TempDir) -> Workspace {
        std::fs::write(dir.path().join("aristo.toml"), "").unwrap();
        Workspace {
            root: dir.path().to_path_buf(),
        }
    }

    fn fixture_critique(ws: &Workspace, id_str: &str) -> AnnotationId {
        let id = AnnotationId::parse(id_str).unwrap();
        let cf = CritiqueFile {
            critique: CritiqueBody {
                critiqued_at_text_hash: Sha256::from_bytes(b"text"),
                produced_at_body_hash: Sha256::from_bytes(b"body"),
                produced_by: "test".into(),
                attempts: 1,
                finding_count: Some(2),
                highest_severity: Some(Severity::Suggest),
                findings: vec![
                    Finding {
                        category: Category::Clarity,
                        severity: Severity::Suggest,
                        rationale: "defensive commentary".into(),
                        suggested_text: None,
                        disposition: None,
                        disposition_note: None,
                        closed_at: None,
                    },
                    Finding {
                        category: Category::Rephrasing,
                        severity: Severity::StrongSuggest,
                        rationale: "double negation".into(),
                        suggested_text: Some("rewrite".into()),
                        disposition: None,
                        disposition_note: None,
                        closed_at: None,
                    },
                ],
            },
        };
        let path = critique_path_for(ws, &id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, cf.to_toml().unwrap()).unwrap();
        id
    }

    #[test]
    fn on_accept_stamps_disposition_accepted() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let id = fixture_critique(&ws, "foo");
        let r = ItemRef::new(id.as_str(), 1);
        CritiqueReviewSession
            .on_accept(&r, Some("ship it"), &ws)
            .unwrap();
        let body = std::fs::read_to_string(critique_path_for(&ws, &id)).unwrap();
        assert!(body.contains("disposition = \"accepted\""));
        assert!(body.contains("ship it"));
    }

    #[test]
    fn on_reject_returns_fingerprint_and_stamps() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let id = fixture_critique(&ws, "foo");
        let r = ItemRef::new(id.as_str(), 0);
        let fp = CritiqueReviewSession.on_reject(&r, None, &ws).unwrap();
        assert_eq!(fp["category"], "clarity");
        assert_eq!(fp["rationale_sketch"], "defensive commentary");
        let body = std::fs::read_to_string(critique_path_for(&ws, &id)).unwrap();
        assert!(body.contains("disposition = \"rejected\""));
    }

    #[test]
    fn on_pending_returns_backlog_snapshot() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let id = fixture_critique(&ws, "foo");
        let r = ItemRef::new(id.as_str(), 1);
        let snap = CritiqueReviewSession
            .on_pending(&r, Some("later"), &ws)
            .unwrap();
        assert_eq!(snap["category"], "rephrasing");
        assert_eq!(snap["severity"], "strong-suggest");
        assert_eq!(snap["suggested_text"], "rewrite");
        let body = std::fs::read_to_string(critique_path_for(&ws, &id)).unwrap();
        assert!(body.contains("disposition = \"deferred\""));
    }

    #[test]
    fn bad_ref_yields_actionable_error() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let r = ItemRef::from_opaque("no-separator-here");
        let err = CritiqueReviewSession.on_accept(&r, None, &ws).unwrap_err();
        assert!(err.to_string().contains("not in"));
    }

    #[test]
    fn missing_critique_file_yields_actionable_error() {
        let tmp = TempDir::new().unwrap();
        let ws = fresh_workspace(&tmp);
        let r = ItemRef::new("never_submitted", 0);
        let err = CritiqueReviewSession.on_accept(&r, None, &ws).unwrap_err();
        assert!(err.to_string().contains("no .critique file"));
    }
}
