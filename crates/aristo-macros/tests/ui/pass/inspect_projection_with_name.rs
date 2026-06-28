//! Projection mode honoring `name = "..."`: the field `entries` is
//! projected to its keys and exposed as `inspect_ids`. `name` is
//! orthogonal to `ret`/`with` and may appear in any order.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect(ret = Vec<u64>, with = project_keys, name = "ids")]
    entries: BTreeMap<u64, u32>,
}

fn project_keys(m: &BTreeMap<u64, u32>) -> Vec<u64> {
    m.keys().copied().collect()
}

fn main() {
    let mut entries = BTreeMap::new();
    entries.insert(8u64, 1u32);
    let s = Store { entries };
    assert_eq!(s.inspect_ids(), vec![8u64]);
}
