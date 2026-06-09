//! `#[expose_pub(as = "...")]` on a `pub(crate)` free function emits
//! the original unchanged plus a `pub` wrapper with the renamed
//! signature, calling through.

mod inner {
    use aristo::instrument::expose_pub;

    #[expose_pub(as = "new_for_test")]
    pub(crate) fn new(buf_size: usize) -> usize {
        buf_size * 2
    }
}

fn main() {
    // The wrapper is reachable from outside the module via the public
    // path. The original `inner::new` stays `pub(crate)` and is also
    // callable from this binary (same crate).
    assert_eq!(inner::new_for_test(4), 8);
    assert_eq!(inner::new(4), 8);
}
