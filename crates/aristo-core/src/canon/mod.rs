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
//! - [`http_client`]: [`HttpCanonClient`] — `ureq`-backed
//!   blocking impl. Pure response-mapping helpers ([`map_response`](
//!   http_client::map_response)) are split from the transport so
//!   every status-code branch is unit-testable without a real
//!   network call.
//! - [`cache`]: [`CanonMatchesFile`] — schema + atomic I/O for
//!   `.aristo/canon-matches.toml`. Three buckets per annotation:
//!   `pending_matches` (surfaced, not reviewed), `accepted_matches`
//!   (bound), `rejected_matches` (suppressed until text changes).
//!   Cache-hit semantics per L5's invalidation rules.
//!
//! **Phase 1 scope**: no verification execution. The `verification`
//! block on [`CanonMatch`](types::CanonMatch) is informational
//! metadata about what Phase 2 will eventually run; the SDK ignores
//! it. See
//! `../aretta-sdk/docs/mockups/13-canon-and-matching/_deferred/verification-execution.md`.

pub mod auth;
pub mod cache;
pub mod client;
pub mod http_client;
pub mod mock_client;
pub mod noop_client;
pub mod rewrite;
pub mod types;

pub use auth::Token;
pub use cache::{
    AcceptedMatch, CacheEntry, CacheMeta, CanonMatchesFile, Disposition, PendingMatch,
    RejectedMatch,
};
pub use client::{AuthError, CanonClient, CanonError};
pub use http_client::{HttpCanonClient, DEFAULT_BASE_URL};
pub use mock_client::MockCanonClient;
pub use noop_client::NoopCanonClient;
pub use rewrite::{compute_rewrite, AcceptRewriteRequest, AttributeRewrite, RewriteError};
pub use types::{
    synthesize_phase1_linked, AnnotationMatchInput, CanonEntry, CanonMatch, CanonMatchRequest,
    CanonMatchResponse, PrefixTier, References, RelatedEntry, RequestVerifyBody,
    RequestVerifyResponse, VerificationMetadata,
};
