//! `verify` must be a literal — non-literal exprs are rejected.

use aristo::intent;

const SOMETHING: bool = true;

#[intent("text", verify = SOMETHING)]
fn nonliteral_verify() -> i32 {
    0
}

fn main() {}
