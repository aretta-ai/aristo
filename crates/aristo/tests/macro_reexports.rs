//! The four annotation macros are reachable via the `aristo::` path.
//!
//! Downstream users add `aristo` to `Cargo.toml` and write `use aristo::intent;`
//! — they should never need to know that the macros physically live in
//! `aristo-macros`. This test asserts the re-exports work for all four
//! surface forms (attribute × function-like, intent × assume).

use aristo::{assume, assume_stmt, intent, intent_stmt};

#[intent("re-exported intent attribute works", verify = "test")]
fn via_intent_attr() -> i32 {
    1
}

#[assume("re-exported assume attribute works", parent = "reexports")]
fn via_assume_attr() -> i32 {
    2
}

fn via_intent_stmt() -> i32 {
    intent_stmt!("re-exported intent_stmt works", verify = true);
    3
}

fn via_assume_stmt() -> i32 {
    assume_stmt!("re-exported assume_stmt works");
    4
}

#[test]
fn all_four_macros_reachable_via_aristo() {
    assert_eq!(via_intent_attr(), 1);
    assert_eq!(via_assume_attr(), 2);
    assert_eq!(via_intent_stmt(), 3);
    assert_eq!(via_assume_stmt(), 4);
}

// Fully-qualified paths also work (no `use` needed at the call site).

#[aristo::intent("FQ path on attribute form")]
fn fq_intent() -> i32 {
    5
}

#[aristo::assume("FQ path on assume form")]
fn fq_assume() -> i32 {
    6
}

#[test]
fn fully_qualified_paths_work() {
    assert_eq!(fq_intent(), 5);
    assert_eq!(fq_assume(), 6);

    let v = {
        aristo::intent_stmt!("FQ path on intent_stmt");
        aristo::assume_stmt!("FQ path on assume_stmt");
        7
    };
    assert_eq!(v, 7);
}
