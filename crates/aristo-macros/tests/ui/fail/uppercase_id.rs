//! Non-snake_case ids are rejected.

use aristo::intent;

#[intent("text", id = "FooBar")]
fn upper() -> i32 {
    0
}

fn main() {}
