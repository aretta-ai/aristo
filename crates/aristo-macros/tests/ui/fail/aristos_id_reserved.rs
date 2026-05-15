//! User-written `aristos:` ids are rejected — that namespace is reserved
//! for `aristo sync`.

use aristo::intent;

#[intent("text", id = "aristos:my_thing")]
fn user_wrote_aristos() -> i32 {
    0
}

fn main() {}
