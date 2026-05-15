//! Unknown `verify` string value is rejected; the error names the allowed set.

use aristo::intent;

#[intent("text", verify = "yolo")]
fn bad_verify() -> i32 {
    0
}

fn main() {}
