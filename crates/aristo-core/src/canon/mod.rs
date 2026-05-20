//! §13 canon-and-matching: client trait, wire types, and impls.
//!
//! See `../aretta-sdk/docs/launch/canon-strategy.md` and
//! `../aretta-sdk/docs/mockups/13-canon-and-matching/` for the
//! design archive (the contract; this module implements it).
//!
//! Module layout:
//!
//! - [`types`]: request/response shapes for the three Phase 1
//!   endpoints (`/canon/match`, `/canon/entry/<id>`,
//!   `/canon/request-verify`). Serialize to JSON (wire) and TOML
//!   (fixtures + on-disk cache).
//! - [`client`]: the [`CanonClient`] trait + [`CanonError`] +
//!   [`AuthError`].
//! - `noop_client`, `mock_client`, `http_client` (added in
//!   subsequent commits / PRs): concrete impls.
//!
//! **Phase 1 scope**: no verification execution. The `verification`
//! block on [`CanonMatch`](types::CanonMatch) is informational
//! metadata about what Phase 2 will eventually run; the SDK ignores
//! it. See
//! `../aretta-sdk/docs/mockups/13-canon-and-matching/_deferred/verification-execution.md`.

pub mod client;
pub mod types;

pub use client::{AuthError, CanonClient, CanonError};
pub use types::{
    AnnotationMatchInput, CanonEntry, CanonMatch, CanonMatchRequest, CanonMatchResponse,
    PrefixTier, References, RelatedEntry, RequestVerifyBody, RequestVerifyResponse,
    VerificationMetadata,
};
