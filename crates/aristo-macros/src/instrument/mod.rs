//! `aristo::instrument` proc-macros (Phase 2).
//!
//! Surfaces — `Inspect` derive, `expose_pub` attribute, `yield_point!`
//! (observe-only) and `fault_point!` (returns a fault `Decision`) — making
//! private state observable to verification harnesses and letting them inject
//! faults.
//!
//! The whole subtree is feature-gated under `aristo_instrument` so
//! consumers who don't use the surface pay no compile cost.

pub(crate) mod expose_pub;
pub(crate) mod fault_point;
pub(crate) mod inspect;
pub(crate) mod yield_point;
