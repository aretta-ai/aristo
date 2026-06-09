//! v1 supports `SkipMap<K, V>` fields only. Tagging any other field
//! type (here a `Vec<T>`) produces a clear macro-level error pointing
//! at the offending field.

use aristo::instrument::Inspect;

#[derive(Inspect)]
pub struct S {
    #[inspect]
    items: Vec<u64>,
}

fn main() {}
