//! `yield_point!("label")` — slice 36 stub.
//!
//! Parses the argument as a string literal (so non-literal labels fail
//! at the call site with a clear `syn` error even before the runtime
//! call lands), then emits nothing. Slice 40 wires the expansion to
//! `aristo::instrument::__yield_point(<label>);` under the
//! `aristo_instrument` feature.

use proc_macro::TokenStream;

pub(crate) fn function_like(input: TokenStream) -> TokenStream {
    let _label = syn::parse_macro_input!(input as syn::LitStr);
    TokenStream::new()
}
