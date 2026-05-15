//! User-written `aret_` ids are rejected — that prefix is reserved for
//! `aristo stamp`.

use aristo::intent;

#[intent("text", id = "aret_my_thing")]
fn user_wrote_aret() -> i32 {
    0
}

fn main() {}
