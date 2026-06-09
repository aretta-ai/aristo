//! `#[expose_pub(as = "...")]` on an `ImplItemFn` — both methods (with
//! `&self` receiver, generics, lifetimes) and associated functions
//! (no receiver, returning `Self`). The wrapper preserves the receiver
//! kind and generic params verbatim; the call shape adapts to receiver
//! presence (`self.X(...)` vs `Self::X(...)`).

mod inner {
    use aristo::instrument::expose_pub;

    pub struct Buf<'a, T> {
        pub items: &'a [T],
    }

    impl<'a, T: Copy> Buf<'a, T> {
        // Method with `&self` + generic `T: Copy` + lifetime `'a`.
        #[expose_pub(as = "first_for_test")]
        pub(crate) fn first(&self) -> Option<T> {
            self.items.first().copied()
        }

        // Associated function (no receiver), returning `Self`.
        #[expose_pub(as = "from_slice_for_test")]
        pub(crate) fn from_slice(items: &'a [T]) -> Self {
            Self { items }
        }
    }
}

fn main() {
    let v = vec![1u32, 2, 3];

    // Associated function wrapper — calls `Self::from_slice(items)`.
    let b = inner::Buf::from_slice_for_test(&v);

    // Method wrapper — calls `self.first()`.
    assert_eq!(b.first_for_test(), Some(1));
}
