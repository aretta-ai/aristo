//! Clone mode: bare `#[inspect]` clones the whole field and returns the
//! field's own declared type. Type-agnostic — no per-collection codegen.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect]
    entries: BTreeMap<u64, String>,
}

fn main() {
    let mut entries = BTreeMap::new();
    entries.insert(1u64, "a".to_string());
    let s = Store { entries };
    let snap: BTreeMap<u64, String> = s.inspect_entries();
    assert_eq!(snap.get(&1).map(String::as_str), Some("a"));
}
