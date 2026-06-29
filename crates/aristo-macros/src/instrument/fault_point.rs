//! `fault_point!("label")` — function-like macro that emits a call into the
//! fault-decision hook, returning the harness's `Decision` for the SUT to
//! branch on.
//!
//! Expansion (when the `aristo_instrument` feature is on):
//!
//! ```ignore
//! aristo::instrument::fault_point!("wal.sync.before_fsync")
//! // expands to:
//! ::aristo::instrument::__fault_point("wal.sync.before_fsync")
//! ```
//!
//! Unlike `yield_point!` — observe-only, returns `()` — `fault_point!` returns
//! a [`Decision`] the SUT matches on to inject a fault (return its own error,
//! drop bytes, …). aristo carries *when*; the SUT owns *what*. Reach for it
//! only for **interior** faults — a point inside one operation with no I/O-seam
//! call to attach the fault to; seam-boundary faults live in the harness's own
//! fault-I/O impl, no macro needed.
//!
//! The label must be a string literal — same `&'static str` discipline as
//! `yield_point!`, keeping the dispatch path allocation-free.
//!
//! [`Decision`]: aristo::instrument::Decision

use proc_macro::TokenStream;
use quote::quote;

pub(crate) fn function_like(input: TokenStream) -> TokenStream {
    let label = syn::parse_macro_input!(input as syn::LitStr);
    quote! {
        ::aristo::instrument::__fault_point(#label)
    }
    .into()
}
