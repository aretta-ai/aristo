//! `aristo::intent_stmt!()` function-like macro: slice 6 pass-through (empty
//! expansion).
//!
//! Used inside fn bodies to attach intent to statements, blocks, or loops
//! that the attribute form can't reach. The macro expands to an empty token
//! stream — purely a compile-time annotation, no runtime trace.

use aristo_macros::intent_stmt;

fn loops_over_pairs(items: &[i32]) -> i32 {
    intent_stmt!(
        "the loop accumulates a sum across all input pairs",
        verify = "test",
        parent = "summation_invariants"
    );

    let mut acc = 0;
    for chunk in items.chunks(2) {
        intent_stmt!("each chunk contributes its two-element sum");
        acc += chunk.iter().sum::<i32>();
    }
    acc
}

fn assignment_intent() -> i32 {
    let n = 5;
    intent_stmt!(
        "x is initialized to twice n",
        verify = true,
        id = "x_eq_two_n"
    );
    let x = 2 * n;
    x + 1
}

#[test]
fn intent_stmt_expands_to_nothing() {
    // The annotations don't affect runtime behavior.
    assert_eq!(loops_over_pairs(&[1, 2, 3, 4, 5, 6]), 1 + 2 + 3 + 4 + 5 + 6);
    assert_eq!(assignment_intent(), 11);
}

#[test]
fn intent_stmt_in_block_position() {
    let result = {
        intent_stmt!("this block computes a constant");
        42
    };
    assert_eq!(result, 42);
}

#[test]
fn intent_stmt_with_trailing_comma() {
    #[rustfmt::skip]
    fn inner() -> i32 {
        intent_stmt!("trailing comma works", verify = "test",);
        7
    }
    assert_eq!(inner(), 7);
}
