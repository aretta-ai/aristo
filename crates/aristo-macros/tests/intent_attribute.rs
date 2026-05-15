//! `#[aristo::intent]` attribute macro: slice 6 pass-through behavior.
//!
//! These tests assert what the macro promises in slice 6: parse the argument
//! shape (text positional + verify/parent/id key=value), and emit the
//! wrapped item completely unchanged. No validation, no expansion. The
//! `aristo_check` slice (8) is what adds validation; the trybuild slice (7)
//! is what locks down compile-fail behavior.

use aristo_macros::intent;

#[intent("the function returns its input plus one")]
fn add_one(x: i32) -> i32 {
    x + 1
}

#[intent("returns 42", verify = "test")]
fn with_verify_string() -> i32 {
    42
}

#[intent("returns 7", verify = true)]
fn with_verify_bool() -> i32 {
    7
}

#[intent("returns 9", verify = "test", parent = "math_invariants")]
fn with_parent_singular() -> i32 {
    9
}

#[intent("returns 11", verify = "test", parent = ["a", "b"])]
fn with_parent_list() -> i32 {
    11
}

#[intent(
    "returns 13",
    verify = "test",
    parent = "math_invariants",
    id = "addition_works"
)]
fn with_explicit_id() -> i32 {
    13
}

#[rustfmt::skip]
#[intent("trailing comma works", verify = "test",)]
fn trailing_comma() -> i32 {
    1
}

#[intent("text only — no other args")]
fn text_only() -> i32 {
    2
}

#[test]
fn intent_attribute_is_pass_through() {
    assert_eq!(add_one(3), 4);
    assert_eq!(with_verify_string(), 42);
    assert_eq!(with_verify_bool(), 7);
    assert_eq!(with_parent_singular(), 9);
    assert_eq!(with_parent_list(), 11);
    assert_eq!(with_explicit_id(), 13);
    assert_eq!(trailing_comma(), 1);
    assert_eq!(text_only(), 2);
}

// ---------------------------------------------------------------------------
// Item-level coverage: per mockup 01 the attribute applies to fn / mod /
// struct / impl / trait / type / const / static / module-root. Slice 6 only
// needs to prove pass-through across these surfaces; trybuild (slice 7) will
// lock down the full matrix.

#[intent("a struct that holds an integer")]
struct Holder {
    value: i32,
}

#[intent("the impl block on Holder")]
impl Holder {
    #[intent("constructor returning a holder with the given value")]
    fn new(value: i32) -> Self {
        Self { value }
    }
}

#[intent("a trait describing things that can be doubled")]
trait Doublable {
    fn doubled(&self) -> i32;
}

#[intent("Holder doubles by multiplying its value by two")]
impl Doublable for Holder {
    fn doubled(&self) -> i32 {
        self.value * 2
    }
}

#[intent("a type alias for clarity")]
type SmallInt = i32;

#[intent("a module grouping math helpers")]
mod math_helpers {
    use aristo_macros::intent;

    #[intent("returns x squared")]
    pub fn square(x: i32) -> i32 {
        x * x
    }
}

#[test]
fn intent_attribute_applies_to_non_fn_items() {
    let h = Holder::new(7);
    assert_eq!(h.doubled(), 14);
    let _: SmallInt = 5;
    assert_eq!(math_helpers::square(4), 16);
}
