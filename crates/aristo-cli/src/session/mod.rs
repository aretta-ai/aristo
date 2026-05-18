//! Generic review-session substrate (slice 27.5).
//!
//! Several aristo workflows produce artifacts the user must triage
//! before they land:
//!
//! - **Critique review** — categorized prose findings the user accepts,
//!   rejects (so they don't recur), or defers.
//! - **Proof review** — neural-verify verdicts + suggested annotations
//!   on inconclusive proofs.
//! - **Authoring review** (future) — agent-proposed annotations during
//!   code-writing.
//!
//! Without an explicit triage substrate, three failure modes recur:
//! slop drift (artifacts commit without sign-off), zombie suggestions
//! (rejected items reappear), and mid-review derailment (agent moves
//! on with a half-finished review open).
//!
//! This module is the generic substrate underneath those flows. The
//! per-kind specialists (CritiqueReviewSession, ProofReviewSession)
//! implement [`SessionKind`] and plug in here; this module owns the
//! `.active` pointer, the strict-vs-defer-undecided exit semantics,
//! the rejection log, and the backlog plumbing.
//!
//! See `docs/decisions/review-sessions.md` for the design discussion
//! (9 decisions D1–D9, 5 parked open questions).
//!
//! ## Wire shape
//!
//! - Session id: ULID (time-orderable; sorts in `ls`).
//! - On-disk: `.aristo/sessions/active/<id>-<kind>.session.toml` while
//!   in flight; moves to `.aristo/sessions/closed/` on exit. The
//!   `.active` pointer file holds the in-flight session's id.
//! - Item ref: opaque per-kind string, `#`-separated (`<id>#<index>`).
//!   `#` was chosen over `:` because annotation ids may themselves
//!   contain colons.
//! - All of `.aristo/sessions/` is gitignored — only the *effects* of
//!   approvals (source edits) reach git.

pub mod backlog;
pub mod id_gen;
pub mod rejections;
pub mod storage;
pub mod types;
