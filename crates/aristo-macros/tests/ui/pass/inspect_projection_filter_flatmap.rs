//! Projection mode can FILTER and FAN-OUT: one map entry maps to N
//! snapshot rows, and entries can be dropped. The pre-0.3.0 per-entry
//! `From<&V>` codegen (one output tuple per entry, keyed by `*e.key()`)
//! physically could not express this. Models the real
//! `inspect_recovered_schema_records` accessor (keep negative keys; fan
//! one entry's `Vec` into one row per element).

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Recovered {
    #[inspect(ret = Vec<(i64, u32)>, with = project_recovered)]
    rows: BTreeMap<i64, Vec<u32>>,
}

fn project_recovered(m: &BTreeMap<i64, Vec<u32>>) -> Vec<(i64, u32)> {
    m.iter()
        .filter(|(k, _)| **k < 0)
        .flat_map(|(k, vs)| vs.iter().map(move |v| (*k, *v)))
        .collect()
}

fn main() {
    let mut rows = BTreeMap::new();
    rows.insert(-1i64, vec![1u32, 2]);
    rows.insert(5i64, vec![9]);
    let r = Recovered { rows };
    assert_eq!(r.inspect_rows(), vec![(-1i64, 1u32), (-1, 2)]);
}
