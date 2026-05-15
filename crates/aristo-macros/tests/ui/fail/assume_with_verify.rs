//! `verify` is forbidden on `assume` (A5) — error points the user at
//! `intent` instead.

use aristo::assume;

#[assume("text", verify = "test")]
fn category_error() -> i32 {
    0
}

fn main() {}
