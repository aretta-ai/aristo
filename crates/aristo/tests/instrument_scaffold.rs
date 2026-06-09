//! Slice 36 smoke test — `aristo::instrument` re-exports resolve and
//! the proc-macro stubs expand cleanly.
//!
//! Real codegen verification (the trybuild matrix) lands per surface in
//! slices 37–40. This test only exercises the scaffold: the macros are
//! reachable at their advertised paths, accept syntactically valid
//! input, and don't break compilation; the runtime hook installs and
//! dispatches correctly.
//!
//! Run via `cargo test -p aristo --features aristo_instrument`. With the
//! feature off, the file compiles to nothing (the whole module is
//! gated), so `cargo test --workspace` stays green by default.

#![cfg(feature = "aristo_instrument")]

use aristo::instrument::{__yield_point, set_hook};
use std::cell::Cell;

#[derive(aristo::instrument::Inspect)]
#[allow(dead_code)]
pub struct StubStruct {
    field: u64,
}

#[aristo::instrument::expose_pub(as = "_stub_wrapper_not_emitted_yet")]
#[allow(dead_code)]
pub(crate) fn stub_fn() -> u64 {
    42
}

fn use_yield_point_macro() {
    aristo::instrument::yield_point!("scaffold.smoke");
}

thread_local! {
    static OBSERVED: Cell<Option<&'static str>> = const { Cell::new(None) };
}

fn capture(label: &'static str) {
    OBSERVED.with(|o| o.set(Some(label)));
}

#[test]
fn scaffold_re_exports_resolve() {
    // Constructing the struct + calling the (non-wrapped) original fn
    // proves the proc-macros emitted code that compiles cleanly. The
    // slice 36 stubs don't generate accessors / wrappers yet, so we
    // don't call `inspect_*()` or `_stub_wrapper_not_emitted_yet()`.
    let _ = StubStruct { field: 0 };
    let _ = stub_fn();
    use_yield_point_macro();
}

#[test]
fn runtime_hook_round_trips_via_meta_crate() {
    OBSERVED.with(|o| o.set(None));
    set_hook(Some(capture));
    __yield_point("scaffold.observed");
    assert_eq!(OBSERVED.with(|o| o.get()), Some("scaffold.observed"));
    set_hook(None);
}
