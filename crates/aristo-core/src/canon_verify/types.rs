//! Wire types for `POST /canon/verify/sessions` and
//! `GET /canon/verify/sessions/:id`.
//!
//! Mirrors `.../mockups/14-canon-verification-execution/WORKFLOW.md` §6 +
//! `PLUGIN-WIRING.md` §4 byte-for-byte. JSON over the wire; field-name
//! casing is snake_case so serde's defaults match.
//!
//! ### Two enums, deliberately distinct (WORKFLOW.md §5 + §6)
//!
//! - [`TestOutcomeStatus`] is the **test-level** result enum the worker
//!   stamps per test_binary execution (`pass | fail | build_failed |
//!   clone_failed | timeout | error`).
//! - [`AnnotationOutcomeStatus`] is the **annotation-level** aggregate
//!   the server computes at GET time per §6 precedence rules
//!   (`verified | failed | build_failed | inconclusive | no_coverage`).
//!
//! Both are reproduced here because the SDK reads them off the GET
//! response to render the CLI / exit-code surface. The mapping
//! direction (test → annotation) lives server-side; the SDK consumes
//! whatever the server already aggregated.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ─── POST /canon/verify/sessions ───────────────────────────────────────────

/// Body for `POST /canon/verify/sessions` (WIRE 1 per WORKFLOW.md §2).
///
/// The SDK collects every eligible canon-bound annotation
/// (`verify = "full"` + `kanon:` / `aristos:` tier) and bundles them
/// into a single session per invocation. The server fans them out
/// across test_binaries using the toolsaurus coverage plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifySessionRequest {
    /// `owner/repo` shape. Derived by the SDK from the workspace's
    /// `.git/config` (or `ARISTO_REPO` env override). The server
    /// re-derives effective scopes from `repository_flavors` keyed
    /// on this string.
    pub repo_full_name: String,
    /// Full SHA of the commit the worker should check out. Derived
    /// by the SDK from `git rev-parse HEAD`. Must already be on
    /// `origin/<branch>` — the SDK's push-first precheck enforces
    /// this client-side; the server does NOT re-verify push-state
    /// (it'll just fail at clone time if absent, surfacing as
    /// `clone_failed`).
    pub commit_sha: String,
    /// Tags to verify in this session. The SDK eligibility filter
    /// has already applied (`verify = "full"` + canon-bound +
    /// `--filter` if any); the server reduces further via
    /// coverage-plan-emptiness per §7b.
    pub tags: Vec<VerifySessionTag>,
}

/// One canon-bound annotation in a session request.
///
/// `annotation_id` is the **server-side opaque ref** (the `linked`
/// `arta_*` ArtaId in the SDK's [`crate::index::BindingState::Bound`]
/// / [`crate::index::BindingState::Certified`]) — NOT the user-chosen
/// [`crate::index::AnnotationId`] (`aristos:foo`). The server uses
/// `annotation_id` to attribute results back; `canon_id` + `version`
/// pin the test_binary lookup via the toolsaurus plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifySessionTag {
    /// Server-issued binding handle (the `arta_*` ref).
    pub annotation_id: String,
    /// Canon entry id (suffix only; no `aristos:` / `kanon:` prefix).
    pub canon_id: String,
    /// Per-entry version pinned at canon-accept time.
    pub version: String,
    /// `<path>:<line>` source location, surfaced in the CLI + Check
    /// Run + dashboard rendering. Server persists it in the plan and
    /// joins it back into the GET response.
    pub source_path: String,
}

/// 202 Accepted body returned by `POST /canon/verify/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PostVerifySessionResponse {
    /// Session identifier; equals `aretta_ci_jobs.job_id` per §7c row
    /// 12. Used as the path parameter for the GET endpoint and shown
    /// to the user.
    pub session_id: String,
    /// Browser-friendly URL the SDK prints alongside `session_id`.
    pub view_url: String,
    /// How many runnable tags landed in the plan (excludes
    /// `no_coverage` tags). The SDK uses this for the "verifying N
    /// annotations" lede.
    pub plan_size: u32,
}

// ─── GET /canon/verify/sessions/:id ────────────────────────────────────────

/// Top-level GET response (WORKFLOW.md §6 — single source of truth
/// for CLI / dashboard / Check Run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetVerifySessionResponse {
    pub session_id: String,
    pub status: SessionStatus,
    pub user_commit_sha: String,
    /// `aretta-books` HEAD at the time the session ran. `None` for
    /// sessions still queued (worker hasn't recorded it yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub books_commit_sha: Option<String>,
    /// Catalog snapshot tag. Informational; mirrors the same field on
    /// canon-match responses.
    pub canon_version: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub annotations: Vec<AnnotationVerification>,
    pub summary: VerifySessionSummary,
}

/// Session-level lifecycle state. Tracks `aretta_ci_jobs.status` enum
/// from the proxy schema (queued | running | done | failed | timed_out
/// | cancelled). Mapped 1:1 — no SDK-side aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Queued,
    Running,
    Done,
    Failed,
    TimedOut,
    Cancelled,
}

impl SessionStatus {
    /// True iff this session is in a terminal state — no further
    /// `?wait=N` poll will produce a status change.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::TimedOut | Self::Cancelled
        )
    }
}

/// Per-annotation block in the GET response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationVerification {
    pub annotation_id: String,
    pub canon_id: String,
    pub version: String,
    pub scope: String,
    /// Source-form prefix string including trailing colon
    /// (`"aristos:"` or `"kanon:"`). Mirrors [`crate::canon::PrefixTier::as_prefix`].
    pub tier: String,
    pub source_path: String,
    /// `https://github.com/<repo>/blob/<sha>/<path>#L<line>` — server
    /// derives from the persisted plan + `commit_sha`. Optional only
    /// to tolerate sessions where the SDK couldn't resolve a line
    /// (defensive — current paths always include `:line`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub status: AnnotationOutcomeStatus,
    pub tests: Vec<TestOutcome>,
}

/// Annotation-level aggregate (WORKFLOW.md §6 precedence:
/// `failed > build_failed > inconclusive > verified`; `no_coverage`
/// is a sentinel for tags with zero matching coverage rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationOutcomeStatus {
    Verified,
    Failed,
    BuildFailed,
    Inconclusive,
    NoCoverage,
}

/// One row in the per-annotation `tests` array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TestOutcome {
    /// Nullable: clone-failure sentinel rows carry `test_binary =
    /// null` (per §6 + WIRE 5 lock).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_binary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_kind: Option<String>,
    /// `tight | partial | informal`. String not enum: the server's
    /// vocabulary is allowed to grow before the SDK pins it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    pub status: TestOutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Lazy-fetch handle. SDK doesn't follow it in E1–E4; dashboard does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_url: Option<String>,
}

/// Test-level outcome enum (WORKFLOW.md §5 — six values, distinct
/// from [`AnnotationOutcomeStatus`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestOutcomeStatus {
    Pass,
    Fail,
    BuildFailed,
    CloneFailed,
    Timeout,
    Error,
}

/// Aggregate counters on the GET response. The SDK's `--wait` exit
/// code is computed exclusively from these (§7c row 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifySessionSummary {
    pub total_annotations: u32,
    pub verified: u32,
    pub failed: u32,
    pub build_failed: u32,
    pub inconclusive: u32,
    pub no_coverage: u32,
}

impl VerifySessionSummary {
    /// §7c row 7: success iff `failed + build_failed + inconclusive == 0`.
    /// `no_coverage` is **informational** and does NOT fail the exit code.
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.build_failed == 0 && self.inconclusive == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── round-trips ─────────────────────────────────────────────────────

    #[test]
    fn post_request_round_trips_through_json() {
        let req = VerifySessionRequest {
            repo_full_name: "tursodatabase/turso".into(),
            commit_sha: "8c31a45deadbeef0000000000000000000000000".into(),
            tags: vec![VerifySessionTag {
                annotation_id: "arta_op4q3z9NbV".into(),
                canon_id: "vacuum_preserves_logical_content".into(),
                version: "v0.1.0".into(),
                source_path: "crates/vacuum/src/lib.rs:42".into(),
            }],
        };
        let s = serde_json::to_string(&req).unwrap();
        let parsed: VerifySessionRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn post_response_round_trip() {
        let resp = PostVerifySessionResponse {
            session_id: "01HN1234567890ABCDEFGHJKMN".into(),
            view_url: "https://dev.aretta.ai/dashboard/jobs/01HN1234567890ABCDEFGHJKMN".into(),
            plan_size: 3,
        };
        let s = serde_json::to_string(&resp).unwrap();
        let parsed: PostVerifySessionResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn get_response_round_trip_matches_workflow_md_sample() {
        // Adapted from WORKFLOW.md §6 example: 3 annotations (1
        // verified, 1 failed, 1 no_coverage).
        let json = r#"{
          "session_id": "01HN...",
          "status": "failed",
          "user_commit_sha": "8c31a45",
          "books_commit_sha": "e1ed19e",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-23T10:00:00Z",
          "completed_at": "2026-05-23T10:05:00Z",
          "annotations": [
            {
              "annotation_id": "arta_a1b2c3",
              "canon_id": "vacuum_preserves_logical_content",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "crates/vacuum/src/lib.rs:42",
              "source_url": "https://github.com/tursodatabase/turso/blob/8c31a45/crates/vacuum/src/lib.rs#L42",
              "status": "verified",
              "tests": [
                {"test_binary": "vacuum_conform", "test_kind": "differential", "relation": "tight", "status": "pass", "duration_ms": 12340, "stderr_url": null}
              ]
            },
            {
              "annotation_id": "arta_d4e5f6",
              "canon_id": "vacuum_atomic_under_crash",
              "version": "v0.1.0",
              "scope": "turso",
              "tier": "aristos:",
              "source_path": "crates/vacuum/src/lib.rs:101",
              "source_url": "https://github.com/tursodatabase/turso/blob/8c31a45/crates/vacuum/src/lib.rs#L101",
              "status": "failed",
              "tests": [
                {"test_binary": "vacuum_conform", "status": "pass", "duration_ms": 9120},
                {"test_binary": "vacuum_crash_conform", "status": "fail", "duration_ms": 22100, "stderr_url": "/canon/verify/sessions/01HN.../results/abc/stderr"}
              ]
            },
            {
              "annotation_id": "arta_g7h8i9",
              "canon_id": "checkout_total_non_negative",
              "version": "v0.1.0",
              "scope": ":vanilla",
              "tier": "kanon:",
              "source_path": "crates/api/src/checkout.rs:88",
              "status": "no_coverage",
              "tests": []
            }
          ],
          "summary": {
            "total_annotations": 3,
            "verified": 1,
            "failed": 1,
            "build_failed": 0,
            "inconclusive": 0,
            "no_coverage": 1
          }
        }"#;
        let parsed: GetVerifySessionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.session_id, "01HN...");
        assert_eq!(parsed.status, SessionStatus::Failed);
        assert_eq!(parsed.summary.total_annotations, 3);
        assert_eq!(parsed.summary.verified, 1);
        assert_eq!(parsed.summary.failed, 1);
        assert_eq!(parsed.summary.no_coverage, 1);
        assert!(!parsed.summary.is_success());

        assert_eq!(parsed.annotations.len(), 3);
        assert_eq!(parsed.annotations[2].status, AnnotationOutcomeStatus::NoCoverage);
        assert_eq!(parsed.annotations[1].tests.len(), 2);
        assert_eq!(parsed.annotations[1].tests[1].status, TestOutcomeStatus::Fail);
    }

    // ─── enum wire-form pins ─────────────────────────────────────────────

    #[test]
    fn session_status_uses_snake_case_wire_form() {
        // Match `aretta_ci_jobs.status` enum exactly. Casing drift
        // here breaks the deploy contract; pin it.
        assert_eq!(
            serde_json::to_string(&SessionStatus::TimedOut).unwrap(),
            "\"timed_out\""
        );
        assert_eq!(
            serde_json::from_str::<SessionStatus>("\"queued\"").unwrap(),
            SessionStatus::Queued
        );
    }

    #[test]
    fn annotation_status_wire_form_pins_no_coverage_underscore_form() {
        assert_eq!(
            serde_json::to_string(&AnnotationOutcomeStatus::NoCoverage).unwrap(),
            "\"no_coverage\""
        );
        assert_eq!(
            serde_json::to_string(&AnnotationOutcomeStatus::BuildFailed).unwrap(),
            "\"build_failed\""
        );
    }

    #[test]
    fn test_status_six_values_round_trip() {
        for (status, wire) in [
            (TestOutcomeStatus::Pass, "\"pass\""),
            (TestOutcomeStatus::Fail, "\"fail\""),
            (TestOutcomeStatus::BuildFailed, "\"build_failed\""),
            (TestOutcomeStatus::CloneFailed, "\"clone_failed\""),
            (TestOutcomeStatus::Timeout, "\"timeout\""),
            (TestOutcomeStatus::Error, "\"error\""),
        ] {
            assert_eq!(serde_json::to_string(&status).unwrap(), wire);
            assert_eq!(serde_json::from_str::<TestOutcomeStatus>(wire).unwrap(), status);
        }
    }

    // ─── §7c row 7 exit-code rule ────────────────────────────────────────

    #[test]
    fn summary_success_only_when_zero_failed_build_failed_inconclusive() {
        // All-zero: success.
        let s = VerifySessionSummary {
            total_annotations: 1,
            verified: 1,
            failed: 0,
            build_failed: 0,
            inconclusive: 0,
            no_coverage: 0,
        };
        assert!(s.is_success());

        // no_coverage is informational — still success.
        let s = VerifySessionSummary {
            total_annotations: 3,
            verified: 1,
            failed: 0,
            build_failed: 0,
            inconclusive: 0,
            no_coverage: 2,
        };
        assert!(s.is_success());

        // Any of failed / build_failed / inconclusive flips to failure.
        for (f, b, i) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
            let s = VerifySessionSummary {
                total_annotations: 1,
                verified: 0,
                failed: f,
                build_failed: b,
                inconclusive: i,
                no_coverage: 0,
            };
            assert!(!s.is_success(), "should fail for {f}/{b}/{i}");
        }
    }

    // ─── terminal-state classification ──────────────────────────────────

    #[test]
    fn session_status_terminal_classification() {
        for s in [SessionStatus::Queued, SessionStatus::Running] {
            assert!(!s.is_terminal(), "{s:?} must not be terminal");
        }
        for s in [
            SessionStatus::Done,
            SessionStatus::Failed,
            SessionStatus::TimedOut,
            SessionStatus::Cancelled,
        ] {
            assert!(s.is_terminal(), "{s:?} must be terminal");
        }
    }

    // ─── optional fields permitted for forward-compat ───────────────────

    #[test]
    fn get_response_tolerates_missing_optional_fields() {
        // books_commit_sha + completed_at can be absent while session
        // is still queued / running. annotations.tests[*].test_kind /
        // relation / duration_ms / stderr_url can be absent on a
        // clone_failed sentinel.
        let json = r#"{
          "session_id": "01HN...",
          "status": "queued",
          "user_commit_sha": "8c31a45",
          "canon_version": "v0.1.0",
          "started_at": "2026-05-23T10:00:00Z",
          "annotations": [],
          "summary": {
            "total_annotations": 0, "verified": 0, "failed": 0,
            "build_failed": 0, "inconclusive": 0, "no_coverage": 0
          }
        }"#;
        let parsed: GetVerifySessionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.status, SessionStatus::Queued);
        assert!(parsed.books_commit_sha.is_none());
        assert!(parsed.completed_at.is_none());
        assert!(parsed.annotations.is_empty());
    }
}
