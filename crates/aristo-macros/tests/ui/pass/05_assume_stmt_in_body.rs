//! `aristo::assume_stmt!()` in statement and block positions.

use aristo::assume_stmt;

fn reads_buffer(buf: &[u8]) -> u8 {
    assume_stmt!(
        "caller has taken the read lock",
        parent = "buffer_concurrency"
    );
    buf.first().copied().unwrap_or(0)
}

fn allocates() -> usize {
    assume_stmt!("allocator returns aligned pointers on this target");
    let ptr_value = 0xdeadbeef_usize;
    ptr_value
}

fn block_assume() -> i32 {
    let v = {
        assume_stmt!("this block runs single-threaded");
        77
    };
    v
}

fn main() {
    assert_eq!(reads_buffer(&[5, 6, 7]), 5);
    assert_eq!(reads_buffer(&[]), 0);
    assert_eq!(allocates(), 0xdeadbeef);
    assert_eq!(block_assume(), 77);
}
