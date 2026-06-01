//! Phase 16 (c) — user-side known-failure waivers.
//!
//! Two halves of one mechanism:
//!
//! 1. **Write** — `aristo verify --accept <canon-id> --because "<reason>"`
//!    records an accepted gap in `.aristo/expectations.toml`
//!    ([`run_accept`]). Validates the id against the live index so a
//!    typo can't write a dead waiver.
//! 2. **Join** — at verify time, [`evaluate`] folds the loaded
//!    expectations over the terminal session snapshot to decide the
//!    process exit code: a waived failure is an *accepted gap* (green); a
//!    waived annotation that now *passes* trips the strict ratchet (red).
//!
//! The waiver keys on the stable prefixed canon id (`{tier}{canon_id}`,
//! e.g. `aristos:foo`) so it survives re-stamps — see
//! [`aristo_core::expectations`].

use aristo_core::canon_verify::{AnnotationOutcomeStatus, GetVerifySessionResponse};
use aristo_core::expectations::ExpectationsFile;
use aristo_core::index::{AnnotationId, IndexFile};

use crate::commands::index::now_rfc3339;
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// The stable waiver key for a server-sent annotation: `{tier}{canon_id}`
/// (e.g. `aristos:foo`). `None` if the pair doesn't parse as a valid id
/// (defensive — server tiers are always `aristos:` / `kanon:`).
pub(crate) fn waiver_key(tier: &str, canon_id: &str) -> Option<AnnotationId> {
    AnnotationId::parse(&format!("{tier}{canon_id}")).ok()
}

/// `aristo verify --accept <canon-id> --because "<reason>"` — write-only.
/// Resolves the id against the index, then upserts the accepted gap into
/// `.aristo/expectations.toml`. Does not dispatch a verification.
pub(crate) fn run_accept(
    ws: &Workspace,
    index: &IndexFile,
    requested: &str,
    reason: &str,
    tracking: Option<&str>,
) -> CliResult<()> {
    let id = resolve_canon_id(index, requested)?;
    let path = ws.expectations_path();
    let mut file = ExpectationsFile::read(&path).map_err(|e| CliError::Other {
        message: format!("failed to read {}: {e}", path.display()),
        exit_code: 1,
    })?;
    file.accept(
        id.clone(),
        reason.to_string(),
        tracking.map(str::to_string),
        now_rfc3339(),
    );
    file.write_atomic(&path).map_err(CliError::Io)?;

    println!("accepted known gap: {}", id.as_str());
    println!("  reason: {reason}");
    if let Some(t) = tracking {
        println!("  tracking: {t}");
    }
    println!();
    println!(
        "  Recorded in .aristo/expectations.toml — commit it. `aristo verify` will report this as a"
    );
    println!(
        "  known gap (not a failure) until the property holds; when it does, verify goes red so you"
    );
    println!("  remember to remove the stale waiver.");
    Ok(())
}

/// Resolve a user-supplied canon id to a canon-bound entry present in the
/// index. Accepts a prefixed id (`aristos:foo`) or a bare suffix (`foo`,
/// tried against both tiers). Rejects opaque `arta_*` refs and ids absent
/// from the index.
fn resolve_canon_id(index: &IndexFile, requested: &str) -> CliResult<AnnotationId> {
    let raw = requested.trim();
    if raw.starts_with("arta_") {
        return Err(CliError::Other {
            message: format!(
                "--accept rejects opaque server ids (got `{raw}`). Pass the source-form canon id, \
                 e.g. `aristos:foo` or just `foo`."
            ),
            exit_code: 2,
        });
    }
    // A prefixed id is tried as-is; a bare suffix is tried against both
    // canon tiers (a workspace never binds the same suffix to both).
    let candidates: Vec<String> = if raw.contains(':') {
        vec![raw.to_string()]
    } else {
        vec![format!("aristos:{raw}"), format!("kanon:{raw}")]
    };
    for cand in &candidates {
        if let Ok(id) = AnnotationId::parse(cand) {
            if id.is_canon_bound() && index.entries.contains_key(&id) {
                return Ok(id);
            }
        }
    }
    Err(CliError::Other {
        message: format!(
            "`{raw}` is not a canon-bound (`aristos:` / `kanon:`) entry in this workspace's index. \
             Only canon-bound properties can be waived — run `aristo list` to see eligible ids."
        ),
        exit_code: 1,
    })
}

/// The waiver-aware verdict over a terminal session snapshot. Drives the
/// `--wait` exit code: red iff any genuine (un-waived) failure, any
/// operational failure (build/inconclusive — never waivable), or any
/// ratchet breach (a waived property that now passes).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WaiverVerdict {
    pub unwaived_failed: u32,
    pub accepted_gaps: u32,
    pub ratchet_breaches: u32,
    pub build_failed: u32,
    pub inconclusive: u32,
}

impl WaiverVerdict {
    pub fn is_red(&self) -> bool {
        self.unwaived_failed > 0
            || self.build_failed > 0
            || self.inconclusive > 0
            || self.ratchet_breaches > 0
    }
}

/// Fold the user's accepted gaps over the session results. Only a
/// property `Failed` is waivable; `BuildFailed` / `Inconclusive` are
/// operational and always count. A waived `Verified` is a ratchet breach.
pub(crate) fn evaluate(
    snapshot: &GetVerifySessionResponse,
    expectations: &ExpectationsFile,
) -> WaiverVerdict {
    let mut v = WaiverVerdict::default();
    for ann in &snapshot.annotations {
        let waived = waiver_key(&ann.tier, &ann.canon_id)
            .map(|id| expectations.is_waived(&id))
            .unwrap_or(false);
        match ann.status {
            AnnotationOutcomeStatus::Failed => {
                if waived {
                    v.accepted_gaps += 1;
                } else {
                    v.unwaived_failed += 1;
                }
            }
            AnnotationOutcomeStatus::Verified => {
                if waived {
                    v.ratchet_breaches += 1;
                }
            }
            AnnotationOutcomeStatus::BuildFailed => v.build_failed += 1,
            AnnotationOutcomeStatus::Inconclusive => v.inconclusive += 1,
            AnnotationOutcomeStatus::NoCoverage => {}
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(ann_status: &str, canon_id: &str, tier: &str) -> GetVerifySessionResponse {
        let json = format!(
            r#"{{
              "session_id": "s", "status": "done", "user_commit_sha": "x",
              "canon_version": "v", "started_at": "t",
              "annotations": [{{
                "annotation_id": "arta_x", "canon_id": "{canon_id}", "version": "v",
                "scope": "turso", "tier": "{tier}", "source_path": "p",
                "status": "{ann_status}", "tests": []
              }}],
              "summary": {{ "total_annotations": 1, "verified": 0, "failed": 0,
                "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }}
            }}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    fn waived(id: &str) -> ExpectationsFile {
        let mut f = ExpectationsFile::default();
        f.accept(
            AnnotationId::parse(id).unwrap(),
            "reason".into(),
            None,
            "t".into(),
        );
        f
    }

    #[test]
    fn unwaived_failure_is_red() {
        let snap = snapshot_with("failed", "foo", "aristos:");
        let v = evaluate(&snap, &ExpectationsFile::default());
        assert_eq!(v.unwaived_failed, 1);
        assert_eq!(v.accepted_gaps, 0);
        assert!(v.is_red());
    }

    #[test]
    fn waived_failure_is_an_accepted_gap_not_red() {
        let snap = snapshot_with("failed", "foo", "aristos:");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.accepted_gaps, 1);
        assert_eq!(v.unwaived_failed, 0);
        assert!(!v.is_red());
    }

    #[test]
    fn waived_pass_trips_the_ratchet_and_is_red() {
        let snap = snapshot_with("verified", "foo", "aristos:");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.ratchet_breaches, 1);
        assert!(v.is_red());
    }

    #[test]
    fn unwaived_pass_is_green() {
        let snap = snapshot_with("verified", "foo", "aristos:");
        let v = evaluate(&snap, &ExpectationsFile::default());
        assert_eq!(v.ratchet_breaches, 0);
        assert!(!v.is_red());
    }

    #[test]
    fn build_failure_is_red_even_when_waived() {
        // Operational failures are never waivable.
        let snap = snapshot_with("build_failed", "foo", "aristos:");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.build_failed, 1);
        assert_eq!(v.accepted_gaps, 0);
        assert!(v.is_red());
    }
}
