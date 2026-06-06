//! Per-kind session protocol (slice 27.5, step 5).
//!
//! The substrate is generic — it stores opaque `ItemRef`s, manages
//! `.active` / closed-session lifecycle, and writes rejection-log /
//! backlog entries. Per-kind specialists plug in via [`SessionKind`]
//! to define what the buckets actually *do* in source terms:
//!
//! - `on_accept` records the user's agreement into the kind's own
//!   on-disk artifact (e.g., the `.critique` file's
//!   `findings[i].disposition = accepted`).
//! - `on_reject` similarly records disagreement AND returns a
//!   per-kind fingerprint the substrate stores in `rejections.log`
//!   for future `matches_prior_rejection` lookups.
//! - `on_pending` similarly records deferral AND returns a per-kind
//!   data payload the substrate stores in the backlog so future
//!   review can render the deferred item without re-running the
//!   producer worker.
//! - `matches_prior_rejection` lets the kind recognize "this is the
//!   same suggestion the user already rejected" against the
//!   fingerprints in `rejections.log`.
//!
//! The trait is object-safe — methods take `&self` + opaque
//! `ItemRef` rather than associated types, so [`kind_for`] returns
//! `Box<dyn SessionKind>` and `decide.rs` dispatches by the session
//! file's stored kind string.

use crate::session::types::{ItemRef, NestingPolicy};
use crate::workspace::Workspace;
use crate::CliResult;

/// One concrete session kind's plug-in surface.
pub trait SessionKind {
    /// Wire name; matches the `kind` field stored in session TOML
    /// (`"critique-review"`, `"proof-review"`, etc.).
    #[allow(
        dead_code,
        reason = "consumed by per-kind diagnostics + status integration in step 9"
    )]
    fn name(&self) -> &'static str;

    /// Whether other-kind sessions can nest inside this one. v0 ships
    /// `Disallow` only.
    #[allow(dead_code, reason = "consumed once nesting opens (design Q4)")]
    fn nesting_policy(&self) -> NestingPolicy {
        NestingPolicy::Disallow
    }

    /// Record the user's acceptance on the kind's own artifact (e.g.,
    /// the `.critique` file). No substrate record is generated for
    /// accept — the *effect* of the decision goes to source/index,
    /// and the session's closed-state audit trail captures the bucket.
    fn on_accept(&self, item_ref: &ItemRef, note: Option<&str>, ws: &Workspace) -> CliResult<()>;

    /// Record rejection on the kind's artifact AND derive the
    /// fingerprint the substrate writes to `rejections.log`. The
    /// fingerprint is opaque per-kind structured data; substrate
    /// stores `serde_json::Value` and does not introspect.
    fn on_reject(
        &self,
        item_ref: &ItemRef,
        note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value>;

    /// Record deferral on the kind's artifact AND derive the data
    /// payload the substrate stores in the per-kind backlog (so
    /// future review can render the deferred item without re-running
    /// the producer worker).
    fn on_pending(
        &self,
        item_ref: &ItemRef,
        note: Option<&str>,
        ws: &Workspace,
    ) -> CliResult<serde_json::Value>;

    /// True if `item_ref` is "the same suggestion" as a previously-
    /// rejected entry's `fingerprint`. Called by the skill body when
    /// seeding a new session, to partition candidate items into the
    /// open vs auto-rejected buckets (design D7).
    #[allow(
        dead_code,
        reason = "consumed by per-kind skill-body seeding in step 7+"
    )]
    fn matches_prior_rejection(
        &self,
        item_ref: &ItemRef,
        prior_fingerprint: &serde_json::Value,
    ) -> bool;
}

/// Look up the kind implementation by its wire name. Returns `None`
/// for unknown kinds — the substrate treats unknown kinds as
/// substrate-only (no per-kind callbacks fire). Currently registered
/// kinds: `critique-review`, `proof-review`, `intent-review`.
pub fn kind_for(name: &str) -> Option<Box<dyn SessionKind>> {
    match name {
        "critique-review" => Some(Box::new(
            crate::commands::critique::session_kind::CritiqueReviewSession,
        )),
        "proof-review" => Some(Box::new(
            crate::commands::verify::session_kind::ProofReviewSession,
        )),
        "intent-review" => Some(Box::new(
            crate::commands::canon::session_kind::IntentReviewSession,
        )),
        _ => None,
    }
}
