//! [`MockVerifyClient`] — in-process recorder/replayer for tests.
//!
//! Unlike [`crate::canon::MockCanonClient`] (file-fixture driven),
//! the verify mock is constructed in-process with pre-canned
//! responses and records every call so tests can assert the exact
//! request body shape the SDK sends. Verify has one or two endpoints
//! per session (POST then optional GET); fixture files would be more
//! overhead than help.
//!
//! Production callers must never wire this up; the type is shipped
//! in the library so `aristo-cli` integration tests can construct
//! it, but every constructor takes explicit canned data and there's
//! no `from_env` hook.

use std::sync::Mutex;

use super::client::{VerifyClient, VerifyError};
use super::types::{GetVerifySessionResponse, PostVerifySessionResponse, VerifySessionRequest};

/// Canned responses + call recorder.
///
/// Cheap to construct in tests:
///
/// ```
/// # use aristo_core::canon_verify::*;
/// let mock = MockVerifyClient::with_post_response(PostVerifySessionResponse {
///     session_id: "01HN...".into(),
///     view_url: "https://dev.aretta.ai/dashboard/jobs/01HN...".into(),
///     plan_size: 1,
/// });
/// let _: &dyn VerifyClient = &mock;
/// ```
pub struct MockVerifyClient {
    post_response: Mutex<Option<Result<PostVerifySessionResponse, VerifyError>>>,
    get_responses: Mutex<Vec<Result<GetVerifySessionResponse, VerifyError>>>,
    posted: Mutex<Vec<VerifySessionRequest>>,
    fetched: Mutex<Vec<(String, Option<u32>)>>,
}

impl MockVerifyClient {
    /// Construct a mock that returns this canned [`PostVerifySessionResponse`]
    /// on the first POST and panics on any subsequent call.
    pub fn with_post_response(resp: PostVerifySessionResponse) -> Self {
        Self {
            post_response: Mutex::new(Some(Ok(resp))),
            get_responses: Mutex::new(Vec::new()),
            posted: Mutex::new(Vec::new()),
            fetched: Mutex::new(Vec::new()),
        }
    }

    /// Construct a mock that returns this canned error on the first POST.
    pub fn with_post_error(err: VerifyError) -> Self {
        Self {
            post_response: Mutex::new(Some(Err(err))),
            get_responses: Mutex::new(Vec::new()),
            posted: Mutex::new(Vec::new()),
            fetched: Mutex::new(Vec::new()),
        }
    }

    /// Construct a mock that replays this sequence of GET responses
    /// (one per `get_session` call). POST always succeeds with a
    /// synthesized default (use [`Self::with_post_response`] when you
    /// need control over the POST result too).
    pub fn with_get_responses(get_responses: Vec<GetVerifySessionResponse>) -> Self {
        Self {
            post_response: Mutex::new(Some(Ok(PostVerifySessionResponse {
                session_id: "mock-session".into(),
                view_url: "https://mock.aretta.ai/dashboard/jobs/mock-session".into(),
                plan_size: 0,
            }))),
            get_responses: Mutex::new(get_responses.into_iter().map(Ok).collect::<Vec<_>>()),
            posted: Mutex::new(Vec::new()),
            fetched: Mutex::new(Vec::new()),
        }
    }

    /// Construct a mock that returns this POST response on the first
    /// POST and replays the GET sequence in order. Useful for testing
    /// the full POST → poll loop end-to-end (E2 `--wait`).
    pub fn with_post_and_gets(
        post: PostVerifySessionResponse,
        get_responses: Vec<GetVerifySessionResponse>,
    ) -> Self {
        Self {
            post_response: Mutex::new(Some(Ok(post))),
            get_responses: Mutex::new(
                get_responses.into_iter().map(Ok).collect::<Vec<_>>(),
            ),
            posted: Mutex::new(Vec::new()),
            fetched: Mutex::new(Vec::new()),
        }
    }

    /// Inspect the requests POSTed so far. Useful for asserting wire-
    /// shape correctness in dispatcher tests.
    pub fn posted_requests(&self) -> Vec<VerifySessionRequest> {
        self.posted.lock().expect("mock mutex").clone()
    }

    /// Inspect `(session_id, wait_seconds)` pairs the SDK has fetched.
    pub fn fetched_sessions(&self) -> Vec<(String, Option<u32>)> {
        self.fetched.lock().expect("mock mutex").clone()
    }
}

impl std::fmt::Debug for MockVerifyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockVerifyClient")
            .field(
                "posted_count",
                &self.posted.lock().map(|v| v.len()).unwrap_or(0),
            )
            .field(
                "fetched_count",
                &self.fetched.lock().map(|v| v.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl VerifyClient for MockVerifyClient {
    fn post_session(
        &self,
        req: &VerifySessionRequest,
    ) -> Result<PostVerifySessionResponse, VerifyError> {
        self.posted.lock().expect("mock mutex").push(req.clone());
        // Replay the canned response. If exhausted, surface a clear
        // panic rather than a silent re-use — test misconfig should
        // fail loudly.
        let mut slot = self.post_response.lock().expect("mock mutex");
        match slot.take() {
            Some(Ok(r)) => Ok(r),
            Some(Err(e)) => Err(e),
            None => panic!("MockVerifyClient: post_session called more than once"),
        }
    }

    fn get_session(
        &self,
        session_id: &str,
        wait_seconds: Option<u32>,
    ) -> Result<GetVerifySessionResponse, VerifyError> {
        self.fetched
            .lock()
            .expect("mock mutex")
            .push((session_id.to_string(), wait_seconds));
        let mut queue = self.get_responses.lock().expect("mock mutex");
        if queue.is_empty() {
            panic!("MockVerifyClient: get_session called but no canned response remains");
        }
        queue.remove(0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        AnnotationOutcomeStatus, GetVerifySessionResponse, SessionStatus, VerifySessionSummary,
        VerifySessionTag,
    };
    use super::*;

    fn one_tag() -> VerifySessionTag {
        VerifySessionTag {
            annotation_id: "arta_x".into(),
            canon_id: "foo".into(),
            version: "v0.1.0".into(),
            source_path: "src/x.rs:1".into(),
        }
    }

    fn done_response(summary: VerifySessionSummary) -> GetVerifySessionResponse {
        GetVerifySessionResponse {
            session_id: "mock-session".into(),
            status: SessionStatus::Done,
            user_commit_sha: "abc".into(),
            books_commit_sha: None,
            canon_version: "v0.1.0".into(),
            started_at: "2026-05-24T00:00:00Z".into(),
            completed_at: Some("2026-05-24T00:01:00Z".into()),
            annotations: vec![],
            summary,
        }
    }

    #[test]
    fn post_records_request_and_returns_canned_response() {
        let mock = MockVerifyClient::with_post_response(PostVerifySessionResponse {
            session_id: "sid".into(),
            view_url: "https://x/y".into(),
            plan_size: 1,
        });
        let req = VerifySessionRequest {
            repo_full_name: "owner/repo".into(),
            commit_sha: "deadbeef".into(),
            tags: vec![one_tag()],
        };
        let r = mock.post_session(&req).unwrap();
        assert_eq!(r.session_id, "sid");
        let posted = mock.posted_requests();
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].repo_full_name, "owner/repo");
        assert_eq!(posted[0].tags.len(), 1);
    }

    #[test]
    #[should_panic(expected = "post_session called more than once")]
    fn post_twice_panics_to_surface_test_misconfig() {
        let mock = MockVerifyClient::with_post_response(PostVerifySessionResponse {
            session_id: "sid".into(),
            view_url: "https://x/y".into(),
            plan_size: 0,
        });
        let req = VerifySessionRequest {
            repo_full_name: "o/r".into(),
            commit_sha: "x".into(),
            tags: vec![],
        };
        mock.post_session(&req).unwrap();
        let _ = mock.post_session(&req);
    }

    #[test]
    fn get_replays_canned_responses_in_order() {
        let r0 = done_response(VerifySessionSummary {
            total_annotations: 0,
            verified: 0,
            failed: 0,
            build_failed: 0,
            inconclusive: 0,
            no_coverage: 0,
        });
        let r1 = done_response(VerifySessionSummary {
            total_annotations: 1,
            verified: 1,
            failed: 0,
            build_failed: 0,
            inconclusive: 0,
            no_coverage: 0,
        });
        let mock = MockVerifyClient::with_get_responses(vec![r0.clone(), r1.clone()]);
        assert_eq!(mock.get_session("sid", None).unwrap(), r0);
        assert_eq!(mock.get_session("sid", Some(30)).unwrap(), r1);
        let fetched = mock.fetched_sessions();
        assert_eq!(
            fetched,
            vec![("sid".into(), None), ("sid".into(), Some(30))]
        );
    }

    #[test]
    fn post_error_propagates() {
        let mock = MockVerifyClient::with_post_error(VerifyError::BadRequest {
            status: 402,
            message: "no_canon_coverage".into(),
        });
        let req = VerifySessionRequest {
            repo_full_name: "o/r".into(),
            commit_sha: "x".into(),
            tags: vec![one_tag()],
        };
        match mock.post_session(&req).unwrap_err() {
            VerifyError::BadRequest { status: 402, .. } => {}
            other => panic!("expected BadRequest 402, got {other:?}"),
        }
        // Even on error, the request is recorded.
        assert_eq!(mock.posted_requests().len(), 1);
    }

    #[test]
    fn placeholder_status_consumed() {
        // Mirror docs/ensure AnnotationOutcomeStatus stays reachable from this module.
        let _ = AnnotationOutcomeStatus::Verified;
    }
}
