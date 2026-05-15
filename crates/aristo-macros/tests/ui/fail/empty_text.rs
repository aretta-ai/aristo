//! Empty annotation text is rejected by `aristo_check`.

use aristo::intent;

#[intent("")]
fn empty() -> i32 {
    0
}

fn main() {}
