//! All validation errors on one invocation surface together — empty text,
//! bad verify, malformed id all appear in a single `cargo build` run.

use aristo::intent;

#[intent("", verify = "yolo", id = "BadId")]
fn three_errors() -> i32 {
    0
}

fn main() {}
