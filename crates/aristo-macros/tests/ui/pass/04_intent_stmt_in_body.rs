//! `aristo::intent_stmt!()` in statement, block, and loop positions.

use aristo::intent_stmt;

fn loops_over_items(items: &[i32]) -> i32 {
    intent_stmt!(
        "sum across all items, no skipping",
        verify = "test",
        parent = "summation"
    );

    let mut total = 0;
    for &x in items {
        intent_stmt!("each item contributes exactly once");
        total += x;
    }
    total
}

fn block_intent() -> i32 {
    let computed = {
        intent_stmt!("this block computes the answer to everything");
        42
    };
    computed
}

fn nested_position() -> i32 {
    let x = 5;
    if x > 0 {
        intent_stmt!("positive branch returns x squared");
        return x * x;
    }
    0
}

fn main() {
    assert_eq!(loops_over_items(&[1, 2, 3, 4]), 10);
    assert_eq!(block_intent(), 42);
    assert_eq!(nested_position(), 25);
}
