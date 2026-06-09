//! `#[expose_pub]` — slice 36 stub.
//!
//! Returns the wrapped item unchanged so call sites that already use
//! the macro on a `pub(crate)` function or type compile cleanly even
//! before the wrapper / twin-declaration codegen lands. Real codegen
//! ships in slice 38 (function form: `pub` wrapper named via
//! `as = "..."`) and slice 39 (type + impl-block forms: cfg-gated
//! `pub` twin declarations).

use proc_macro::TokenStream;

pub(crate) fn attribute(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
