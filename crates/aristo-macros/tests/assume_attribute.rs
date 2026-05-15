//! `#[aristo::assume]` attribute macro: slice 6 pass-through behavior.
//!
//! `assume` mirrors `intent` minus the `verify` key (A5: assumptions are not
//! verification targets). These tests prove pass-through across the same
//! item surfaces as `intent` and that `verify` is rejected at parse time
//! with a friendly category-error message.

use aristo_macros::assume;

#[assume("the host OS guarantees mmap pages are zero-initialized")]
fn requires_zero_pages() -> u8 {
    0
}

#[assume("upstream caller has held the rwlock", parent = "lock_protocol")]
fn under_lock() -> i32 {
    42
}

#[assume(
    "BTreeMap maintains sort order across inserts",
    id = "btree_sort_order"
)]
fn relies_on_btree() -> i32 {
    7
}

#[assume(
    "memory allocator returns aligned pointers",
    parent = "alignment_invariants",
    id = "allocator_aligns"
)]
fn aligned_alloc_assumed() -> i32 {
    9
}

#[rustfmt::skip]
#[assume("trailing comma works",)]
fn trailing_comma() -> i32 {
    1
}

#[test]
fn assume_attribute_is_pass_through() {
    assert_eq!(requires_zero_pages(), 0);
    assert_eq!(under_lock(), 42);
    assert_eq!(relies_on_btree(), 7);
    assert_eq!(aligned_alloc_assumed(), 9);
    assert_eq!(trailing_comma(), 1);
}

// Item-level coverage parallels intent_attribute.rs.

#[assume("a struct that survives across thread boundaries")]
struct ThreadSafe {
    value: i32,
}

#[assume("ThreadSafe is Send + Sync because all fields are")]
impl ThreadSafe {
    fn new(value: i32) -> Self {
        Self { value }
    }
    fn get(&self) -> i32 {
        self.value
    }
}

#[assume("a trait whose impls promise interior mutability is sound")]
trait InteriorMutable {
    fn touch(&self);
}

#[assume("ThreadSafe has trivial interior mutability (none)")]
impl InteriorMutable for ThreadSafe {
    fn touch(&self) {
        let _ = self.value;
    }
}

#[assume("a module grouping concurrency-related contracts")]
mod concurrency {
    use aristo_macros::assume;

    #[assume("returns the same value across calls (referentially transparent)")]
    pub fn pure_constant() -> i32 {
        100
    }
}

#[test]
fn assume_attribute_applies_to_non_fn_items() {
    let t = ThreadSafe::new(5);
    assert_eq!(t.get(), 5);
    t.touch();
    assert_eq!(concurrency::pure_constant(), 100);
}
