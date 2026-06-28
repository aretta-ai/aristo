//! Projection mode with a named free-function projector. `ret` is the
//! verbatim return type; `with` names any `Fn(&FieldType) -> ret`.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect(ret = Vec<(u64, u32)>, with = project_lens)]
    lens: BTreeMap<u64, u32>,
}

fn project_lens(m: &BTreeMap<u64, u32>) -> Vec<(u64, u32)> {
    m.iter().map(|(k, v)| (*k, *v)).collect()
}

fn main() {
    let mut lens = BTreeMap::new();
    lens.insert(2u64, 20u32);
    let s = Store { lens };
    assert_eq!(s.inspect_lens(), vec![(2u64, 20u32)]);
}
