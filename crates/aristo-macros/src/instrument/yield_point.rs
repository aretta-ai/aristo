//! `yield_point!("label")` — function-like macro that emits a call
//! into the runtime hook defined in the meta-crate.
//!
//! Expansion (when the `aristo_instrument` feature is on at the
//! aristo-macros build level):
//!
//! ```ignore
//! aristo::instrument::yield_point!("write_header.before_fsync");
//! // expands to:
//! ::aristo::instrument::__yield_point("write_header.before_fsync");
//! ```
//!
//! When the feature is off, the macro itself isn't exported (the whole
//! `instrument` proc-macro subtree is gated), so the user gates the
//! call site with `#[cfg(feature = "...")]` / `cfg!()` to keep it
//! truly zero-cost in production builds.
//!
//! The label must be a string literal — required by the runtime hook's
//! `&'static str` signature, which keeps the hot path allocation-free.

use proc_macro::TokenStream;
use quote::quote;

pub(crate) fn function_like(input: TokenStream) -> TokenStream {
    let label = syn::parse_macro_input!(input as syn::LitStr);
    quote! {
        ::aristo::instrument::__yield_point(#label)
    }
    .into()
}
