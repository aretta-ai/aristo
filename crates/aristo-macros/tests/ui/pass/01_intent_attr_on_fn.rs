//! `#[aristo::intent]` on free functions — the most common surface.

#[aristo::intent("the function returns the input plus one")]
fn add_one(x: i32) -> i32 {
    x + 1
}

#[aristo::intent("returns 42", verify = "test")]
fn returns_42() -> i32 {
    42
}

#[aristo::intent("with all args", verify = "test", parent = "math", id = "documented")]
fn fully_specified() -> i32 {
    7
}

fn main() {
    assert_eq!(add_one(3), 4);
    assert_eq!(returns_42(), 42);
    assert_eq!(fully_specified(), 7);
}
