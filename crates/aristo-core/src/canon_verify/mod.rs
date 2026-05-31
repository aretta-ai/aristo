//! §14 canon-verification execution — SDK side.
//!
//! Wires the verification surface that §13 deferred to a follow-up
//! (`.../mockups/13-canon-and-matching/_deferred/verification-execution.md`).
//! Design source of truth: `.../mockups/14-canon-verification-execution/`.
//!
//! ## Module layout
//!
//! - [`types`] — wire shapes for `POST /canon/verify/sessions` and
//!   `GET /canon/verify/sessions/:id`. JSON over the wire.
//! - [`report`] — the Phase 16 typed, relation-kind-polymorphic
//!   [`DifferentialReport`] verify-result record (nests at
//!   `TestOutcome::report`).
//! - [`client`] — the [`VerifyClient`] trait + [`VerifyError`].
//! - [`http_client`] — production [`HttpVerifyClient`] backed by `ureq`.
//! - [`mock_client`] — fixture-driven [`MockVerifyClient`] for tests.
//!
//! Scope re-uses the existing canon auth/token resolution; tokens
//! resolve via [`crate::auth::resolve_full`]. The SDK calls the
//! verify endpoints only when the user is authenticated AND has
//! `kanon:` / `aristos:` annotations with `verify = "full"`.

pub mod client;
pub mod http_client;
pub mod mock_client;
pub mod report;
pub mod types;

pub use client::{VerifyClient, VerifyError};
pub use http_client::HttpVerifyClient;
pub use mock_client::MockVerifyClient;
pub use report::{
    DifferentialReport, ExpectedToFail, FaultSpec, FieldDivergence, Finding, NamedField, OpEvent,
    Property, Relation, Scenario, ShrinkStats, Snapshot, SourceRef, Verdict,
};
pub use types::{
    AnnotationOutcomeStatus, AnnotationVerification, GetVerifySessionResponse,
    PostVerifySessionResponse, SessionStatus, TestOutcome, TestOutcomeStatus, VerifySessionRequest,
    VerifySessionSummary, VerifySessionTag,
};
