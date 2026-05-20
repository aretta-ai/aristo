//! [`NoopCanonClient`] — free-tier and opt-out path.
//!
//! Returns [`CanonError::NotEnabled`] for every method. Callers
//! constructing a [`CanonClient`](super::CanonClient) at startup
//! pick this impl when:
//!
//! - The user is on the free tier (no auth token, no upgrade).
//! - `aristo.toml [canon] enabled = false` (regulated buyers,
//!   air-gapped CI).
//! - Build configuration explicitly disables canon (uncommon; the
//!   feature isn't gated by a cargo feature in Phase 1).
//!
//! The point is that callers can hold a `Box<dyn CanonClient>` and
//! always-`?`-up the error, without branching on
//! `Option<dyn CanonClient>`. The `NotEnabled` variant carries no
//! payload; the calling command (`stamp`, `critique`, `canon show`)
//! decides whether to surface a one-line nudge ("canon is a Pro
//! feature; …") or skip silently.

use super::client::{CanonClient, CanonError};
use super::types::{
    CanonEntry, CanonMatchRequest, CanonMatchResponse, RequestVerifyBody, RequestVerifyResponse,
};

/// No-op canon client. Every method returns [`CanonError::NotEnabled`].
///
/// Construction is zero-cost; no allocation, no resources held.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCanonClient;

impl CanonClient for NoopCanonClient {
    fn match_annotations(
        &self,
        _req: &CanonMatchRequest,
    ) -> Result<CanonMatchResponse, CanonError> {
        Err(CanonError::NotEnabled)
    }

    fn get_entry(&self, _canon_id: &str, _version: Option<&str>) -> Result<CanonEntry, CanonError> {
        Err(CanonError::NotEnabled)
    }

    fn request_verify(
        &self,
        _body: &RequestVerifyBody,
    ) -> Result<RequestVerifyResponse, CanonError> {
        Err(CanonError::NotEnabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::types::AnnotationMatchInput;

    #[test]
    fn match_annotations_returns_not_enabled() {
        let client = NoopCanonClient;
        let req = CanonMatchRequest {
            annotations: vec![AnnotationMatchInput {
                annotation_text: "test".into(),
                applies_to: vec!["fn".into()],
            }],
            confidence_threshold: 0.85,
        };
        let err = client.match_annotations(&req).unwrap_err();
        assert!(matches!(err, CanonError::NotEnabled));
    }

    #[test]
    fn get_entry_returns_not_enabled() {
        let client = NoopCanonClient;
        let err = client.get_entry("any_id", None).unwrap_err();
        assert!(matches!(err, CanonError::NotEnabled));
    }

    #[test]
    fn request_verify_returns_not_enabled() {
        let client = NoopCanonClient;
        let err = client
            .request_verify(&RequestVerifyBody {
                canon_id: "any".into(),
                notes: None,
            })
            .unwrap_err();
        assert!(matches!(err, CanonError::NotEnabled));
    }

    #[test]
    fn noop_client_is_object_safe() {
        // Compile-time check: trait can be used dyn-style. Critical
        // for the `Box<dyn CanonClient>` consumer pattern.
        let _boxed: Box<dyn CanonClient> = Box::new(NoopCanonClient);
    }
}
