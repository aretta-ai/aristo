//! `#[expose_pub]` on a `pub(crate)` enum raises its visibility to
//! `pub` in place (with `#[doc(hidden)]`), so the enum is reachable
//! from across-crate consumers (typically test harnesses) when the
//! macro is active.
//!
//! Type / impl-block forms forbid `as = "..."` because renaming a
//! type breaks every reference. The fail case
//! `expose_pub_type_extra_as` pins that error.

mod inner {
    use aristo::instrument::expose_pub;

    #[expose_pub]
    pub(crate) enum ParsedOp {
        Get(u64),
        Put(u64, Vec<u8>),
    }

    impl ParsedOp {
        pub fn key(&self) -> u64 {
            match self {
                ParsedOp::Get(k) | ParsedOp::Put(k, _) => *k,
            }
        }
    }
}

fn main() {
    // The macro raised ParsedOp's visibility to `pub`; constructing
    // and accessing it from outside `inner` works.
    let op = inner::ParsedOp::Put(7, vec![1, 2, 3]);
    assert_eq!(op.key(), 7);
}
