//! Typo / unknown key on `intent` is rejected with the allowed set listed.

use aristo::intent;

#[intent("text", widget = "wat")]
fn typo() -> i32 {
    0
}

fn main() {}
