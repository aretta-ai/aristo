//! `#[derive(Inspect)]` — slice 36 stub.
//!
//! Parses input as `syn::DeriveInput` so syntactically broken targets
//! still surface a clear diagnostic at the derive site, then emits no
//! derived items. The accessor codegen — emitting `inspect_<field>()`
//! returning `Vec<(K, Snapshot)>` for `SkipMap<K, V>` fields tagged
//! with `#[inspect(snapshot = T, name = "...")]` — lands in slice 37.

use proc_macro::TokenStream;

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    let _input = syn::parse_macro_input!(input as syn::DeriveInput);
    TokenStream::new()
}
