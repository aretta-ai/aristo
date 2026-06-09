//! `#[expose_pub]` on a function REQUIRES `as = "<wrapper_name>"`.
//! Without it the macro can't choose a name distinct from the original,
//! so the wrapper would collide. The error points at the attribute
//! call site with a help message suggesting a name.

use aristo::instrument::expose_pub;

#[expose_pub]
pub(crate) fn new() -> u64 {
    0
}

fn main() {}
