//! `aristo::instrument` proc-macros (Phase 2).
//!
//! Three surfaces — `Inspect` derive, `expose_pub` attribute,
//! `yield_point!` function-like — making private state observable to
//! verification harnesses. Slice 36 ships stubs that compile cleanly and
//! expand to a no-op (or pass through the wrapped item unchanged); real
//! codegen lands in slices 37–40 per `docs/ROADMAP.md` Phase 2.
//!
//! The whole subtree is feature-gated under `aristo_instrument` so
//! consumers who don't use the surface pay no compile cost.

pub(crate) mod expose_pub;
pub(crate) mod inspect;
pub(crate) mod yield_point;
