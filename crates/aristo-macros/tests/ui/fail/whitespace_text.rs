//! Whitespace-only annotation text is rejected.

use aristo::intent;

#[intent("   \t  ")]
fn whitespace() -> i32 {
    0
}

fn main() {}
