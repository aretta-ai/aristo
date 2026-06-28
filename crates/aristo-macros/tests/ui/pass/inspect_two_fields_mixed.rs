//! One struct mixing a clone-mode field and a projection-mode field. Each
//! field's mode is chosen independently at its own `#[inspect]` tag.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Mixed {
    #[inspect]
    count: usize,
    #[inspect(ret = Vec<u64>, with = |m: &BTreeMap<u64, u32>| m.keys().copied().collect())]
    index: BTreeMap<u64, u32>,
}

fn main() {
    let mut index = BTreeMap::new();
    index.insert(3u64, 30u32);
    let m = Mixed { count: 1, index };
    assert_eq!(m.inspect_count(), 1usize);
    assert_eq!(m.inspect_index(), vec![3u64]);
}
