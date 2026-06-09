//! `#[expose_pub]` on a type FORBIDS `as = "..."` — renaming a type
//! would break every reference to it across the crate. The macro
//! emits a sibling pub declaration with the SAME name (just visibility
//! raised), so a rename arg is fundamentally incompatible.

use aristo::instrument::expose_pub;

#[expose_pub(as = "RenamedParsedOp")]
pub(crate) enum ParsedOp {
    A,
    B,
}

fn main() {}
