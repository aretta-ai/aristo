//! `#[expose_pub]` on an `impl` block raises visibility on every method
//! inside, in place (each method becomes `pub` + `#[doc(hidden)]`).
//! Non-method items (associated consts, associated types) are left
//! unchanged. The `as = "..."` arg is forbidden — see the type-form
//! fail case for the corresponding diagnostic.

mod inner {
    use aristo::instrument::expose_pub;

    pub struct Counter {
        pub n: u64,
    }

    #[expose_pub]
    impl Counter {
        pub(crate) fn bump(&mut self) {
            self.n += 1;
        }
        pub(crate) fn read(&self) -> u64 {
            self.n
        }
        // Untouched non-method items still work — `expose_pub` only
        // raises visibility on `fn` items.
        pub(crate) const ZERO: u64 = 0;
    }
}

fn main() {
    let mut c = inner::Counter { n: 0 };
    c.bump();
    c.bump();
    assert_eq!(c.read(), 2);
}
