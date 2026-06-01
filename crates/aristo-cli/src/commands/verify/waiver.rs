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

use aristo_core::canon_verify::{
    AnnotationOutcomeStatus, GetVerifySessionResponse, SessionStatus, TestOutcome,
    TestOutcomeStatus,
};
use aristo_core::expectations::ExpectationsFile;
use aristo_core::index::{AnnotationId, IndexFile};

use crate::commands::index::now_rfc3339;
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

/// The stable waiver key for a server-sent annotation: `{tier}{canon_id}`
/// (e.g. `aristos:foo`). Returns `None` for anything that isn't one of the
/// two real canon tiers, so a malformed/colon-less server `tier` yields a
/// true non-match instead of a *wrong* (Local-namespaced) key that would
/// silently fail to match. `canon_id` is trimmed to mirror the write side.
pub(crate) fn waiver_key(tier: &str, canon_id: &str) -> Option<AnnotationId> {
    if tier != "aristos:" && tier != "kanon:" {
        return None;
    }
    AnnotationId::parse(&format!("{tier}{}", canon_id.trim())).ok()
}

/// True iff any test row carries an *operational* failure (build / clone
/// failure, timeout, error) as opposed to a property `fail`. Operational
/// failures are never waivable — a property waiver must not mask a broken
/// build hiding in the same annotation's test set.
pub(crate) fn tests_operationally_broken(tests: &[TestOutcome]) -> bool {
    tests.iter().any(|t| {
        matches!(
            t.status,
            TestOutcomeStatus::BuildFailed
                | TestOutcomeStatus::CloneFailed
                | TestOutcomeStatus::Timeout
                | TestOutcomeStatus::Error
        )
    })
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
    // A reasonless waiver is how baselines rot — reject an empty / blank
    // reason even though clap already enforced the flag's *presence*.
    if reason.trim().is_empty() {
        return Err(CliError::Other {
            message: "--because requires a non-empty reason. A reasonless waiver is how \
                      baselines rot; explain why this gap is accepted."
                .into(),
            exit_code: 2,
        });
    }
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
/// `--wait` exit code.
///
/// Crucially this reconciles against the **authoritative session summary**,
/// not just the per-annotation fold: `unwaived_failed` is the summary's
/// failure count minus the gaps the user *cleanly* waived, so a snapshot
/// with an empty / partial `annotations` array but a non-zero
/// `summary.failed` still goes red. A session that didn't reach `Done`, or
/// any operational failure (build / inconclusive — never waivable), is red
/// independently.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WaiverVerdict {
    pub unwaived_failed: u32,
    pub accepted_gaps: u32,
    pub ratchet_breaches: u32,
    pub build_failed: u32,
    pub inconclusive: u32,
    pub incomplete_session: bool,
}

impl WaiverVerdict {
    pub fn is_red(&self) -> bool {
        self.unwaived_failed > 0
            || self.build_failed > 0
            || self.inconclusive > 0
            || self.ratchet_breaches > 0
            || self.incomplete_session
    }
}

/// Whether an annotation is a *cleanly* waived gap: the user waived it, it
/// failed at the property level, and it carries no operational test
/// failure. Only these are subtracted from the red exit.
fn is_clean_accepted_gap(
    ann: &aristo_core::canon_verify::AnnotationVerification,
    expectations: &ExpectationsFile,
) -> bool {
    matches!(ann.status, AnnotationOutcomeStatus::Failed)
        && !tests_operationally_broken(&ann.tests)
        && waiver_key(&ann.tier, &ann.canon_id)
            .map(|id| expectations.is_waived(&id))
            .unwrap_or(false)
}

/// Fold the user's accepted gaps over the session results. Only a *cleanly*
/// waived property `Failed` is forgiven; `BuildFailed` / `Inconclusive`
/// (and an operational test hiding under a waived `Failed`) always count. A
/// waived `Verified` is a ratchet breach.
pub(crate) fn evaluate(
    snapshot: &GetVerifySessionResponse,
    expectations: &ExpectationsFile,
) -> WaiverVerdict {
    let mut accepted_gaps = 0u32;
    let mut ratchet_breaches = 0u32;
    for ann in &snapshot.annotations {
        if is_clean_accepted_gap(ann, expectations) {
            accepted_gaps += 1;
            continue;
        }
        let waived = waiver_key(&ann.tier, &ann.canon_id)
            .map(|id| expectations.is_waived(&id))
            .unwrap_or(false);
        if waived && matches!(ann.status, AnnotationOutcomeStatus::Verified) {
            ratchet_breaches += 1;
        }
    }
    let s = &snapshot.summary;
    WaiverVerdict {
        // The summary is authoritative for failures; only the gaps we
        // cleanly waived are forgiven. A partial/empty annotations array
        // can never hide a non-zero summary.failed.
        unwaived_failed: s.failed.saturating_sub(accepted_gaps),
        accepted_gaps,
        ratchet_breaches,
        build_failed: s.build_failed,
        inconclusive: s.inconclusive,
        incomplete_session: !matches!(snapshot.status, SessionStatus::Done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary_for(ann_status: &str) -> &'static str {
        match ann_status {
            "verified" => {
                r#"{ "total_annotations": 1, "verified": 1, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#
            }
            "failed" => {
                r#"{ "total_annotations": 1, "verified": 0, "failed": 1, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#
            }
            "build_failed" => {
                r#"{ "total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 1, "inconclusive": 0, "no_coverage": 0 }"#
            }
            _ => {
                r#"{ "total_annotations": 1, "verified": 0, "failed": 0, "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }"#
            }
        }
    }

    /// One-annotation terminal (`done`) snapshot whose summary is kept
    /// consistent with the annotation status (as the server guarantees).
    fn snapshot_with(
        ann_status: &str,
        canon_id: &str,
        tier: &str,
        tests: &str,
    ) -> GetVerifySessionResponse {
        let summary = summary_for(ann_status);
        let json = format!(
            r#"{{
              "session_id": "s", "status": "done", "user_commit_sha": "x",
              "canon_version": "v", "started_at": "t",
              "annotations": [{{
                "annotation_id": "arta_x", "canon_id": "{canon_id}", "version": "v",
                "scope": "turso", "tier": "{tier}", "source_path": "p",
                "status": "{ann_status}", "tests": {tests}
              }}],
              "summary": {summary}
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
        let snap = snapshot_with("failed", "foo", "aristos:", "[]");
        let v = evaluate(&snap, &ExpectationsFile::default());
        assert_eq!(v.unwaived_failed, 1);
        assert_eq!(v.accepted_gaps, 0);
        assert!(v.is_red());
    }

    #[test]
    fn waived_failure_is_an_accepted_gap_not_red() {
        let snap = snapshot_with("failed", "foo", "aristos:", "[]");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.accepted_gaps, 1);
        assert_eq!(v.unwaived_failed, 0);
        assert!(!v.is_red());
    }

    #[test]
    fn waived_pass_trips_the_ratchet_and_is_red() {
        let snap = snapshot_with("verified", "foo", "aristos:", "[]");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.ratchet_breaches, 1);
        assert!(v.is_red());
    }

    #[test]
    fn unwaived_pass_is_green() {
        let snap = snapshot_with("verified", "foo", "aristos:", "[]");
        let v = evaluate(&snap, &ExpectationsFile::default());
        assert_eq!(v.ratchet_breaches, 0);
        assert!(!v.is_red());
    }

    #[test]
    fn build_failure_is_red_even_when_waived() {
        // Operational failures are never waivable.
        let snap = snapshot_with("build_failed", "foo", "aristos:", "[]");
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.build_failed, 1);
        assert_eq!(v.accepted_gaps, 0);
        assert!(v.is_red());
    }

    #[test]
    fn empty_annotations_with_summary_failure_is_still_red() {
        // Regression guard: the exit verdict must trust the authoritative
        // summary, not just the per-annotation fold. A terminal snapshot
        // with no annotation detail but summary.failed > 0 is RED.
        let json = r#"{
          "session_id": "s", "status": "done", "user_commit_sha": "x",
          "canon_version": "v", "started_at": "t", "annotations": [],
          "summary": { "total_annotations": 1, "verified": 0, "failed": 1,
            "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }
        }"#;
        let snap: GetVerifySessionResponse = serde_json::from_str(json).unwrap();
        let v = evaluate(&snap, &ExpectationsFile::default());
        assert_eq!(v.unwaived_failed, 1);
        assert!(v.is_red());
    }

    #[test]
    fn non_done_terminal_session_is_red() {
        // A session that failed / timed out / was cancelled is red even
        // when it carries no annotation detail and a zeroed summary.
        for status in ["failed", "timed_out", "cancelled"] {
            let json = format!(
                r#"{{
                  "session_id": "s", "status": "{status}", "user_commit_sha": "x",
                  "canon_version": "v", "started_at": "t", "annotations": [],
                  "summary": {{ "total_annotations": 0, "verified": 0, "failed": 0,
                    "build_failed": 0, "inconclusive": 0, "no_coverage": 0 }}
                }}"#
            );
            let snap: GetVerifySessionResponse = serde_json::from_str(&json).unwrap();
            let v = evaluate(&snap, &ExpectationsFile::default());
            assert!(v.incomplete_session, "{status} must be incomplete");
            assert!(v.is_red(), "{status} session must be red");
        }
    }

    #[test]
    fn waived_failed_with_operational_test_is_not_forgiven() {
        // A property waiver must not mask an operational (build) failure
        // hiding in the same annotation's test set.
        let tests = r#"[{ "test_binary": "t1", "status": "fail" },
                        { "test_binary": "t2", "status": "build_failed" }]"#;
        let snap = snapshot_with("failed", "foo", "aristos:", tests);
        let v = evaluate(&snap, &waived("aristos:foo"));
        assert_eq!(v.accepted_gaps, 0, "operational break is not a clean gap");
        assert_eq!(v.unwaived_failed, 1);
        assert!(v.is_red());
    }

    #[test]
    fn waiver_key_rejects_colonless_tier() {
        // A malformed (colon-less) tier yields None, not a wrong Local key.
        assert!(waiver_key("aristos", "foo").is_none());
        assert_eq!(
            waiver_key("aristos:", "foo").unwrap().as_str(),
            "aristos:foo"
        );
        assert_eq!(waiver_key("kanon:", "bar").unwrap().as_str(), "kanon:bar");
    }
}
