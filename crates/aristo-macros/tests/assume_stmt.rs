//! `aristo::assume_stmt!()` function-like macro: slice 6 pass-through (empty
//! expansion).
//!
//! Mirrors `intent_stmt!` minus `verify` (A5). Used inside fn bodies to
//! attach assumptions to statements, blocks, or loops.

use aristo_macros::assume_stmt;

fn reads_shared_buffer(buf: &[u8]) -> u8 {
    assume_stmt!(
        "the caller has acquired the read lock on the buffer",
        parent = "buffer_concurrency"
    );
    buf.first().copied().unwrap_or(0)
}

fn allocates_aligned() -> usize {
    let raw = 0usize;
    assume_stmt!(
        "system allocator returns 16-byte aligned pointers on this target",
        id = "alloc_alignment"
    );
    raw
}

#[test]
fn assume_stmt_expands_to_nothing() {
    assert_eq!(reads_shared_buffer(&[7]), 7);
    assert_eq!(reads_shared_buffer(&[]), 0);
    assert_eq!(allocates_aligned(), 0);
}

#[test]
fn assume_stmt_in_block_position() {
    let result = {
        assume_stmt!("this block runs under a held lock");
        99
    };
    assert_eq!(result, 99);
}
