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
//! - [`noop_client`]: [`NoopCanonClient`] — free-tier / opt-out
//!   path; every method returns [`CanonError::NotEnabled`].
//! - [`mock_client`]: [`MockCanonClient`] — fixture-driven for
//!   tests; reads canned TOML from `ARISTO_CANON_FIXTURE` or an
//!   explicit path.
//! - [`auth`]: token resolution and persistence — env var
//!   (`ARETTA_TOKEN`) → `~/.config/aristo/credentials` →
//!   [`AuthError::NoToken`]. The [`Token`](auth::Token) newtype
//!   redacts itself in `Debug` output to prevent accidental
//!   logging of credentials.
//! - `http_client` (next commit): the real HTTP-backed impl.
//!
//! **Phase 1 scope**: no verification execution. The `verification`
//! block on [`CanonMatch`](types::CanonMatch) is informational
//! metadata about what Phase 2 will eventually run; the SDK ignores
//! it. See
//! `../aretta-sdk/docs/mockups/13-canon-and-matching/_deferred/verification-execution.md`.

pub mod auth;
pub mod client;
pub mod mock_client;
pub mod noop_client;
pub mod types;

pub use auth::Token;
pub use client::{AuthError, CanonClient, CanonError};
pub use mock_client::MockCanonClient;
pub use noop_client::NoopCanonClient;
pub use types::{
    AnnotationMatchInput, CanonEntry, CanonMatch, CanonMatchRequest, CanonMatchResponse,
    PrefixTier, References, RelatedEntry, RequestVerifyBody, RequestVerifyResponse,
    VerificationMetadata,
};
