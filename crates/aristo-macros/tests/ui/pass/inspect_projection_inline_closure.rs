//! Projection mode with an UN-annotated inline closure. The macro's
//! `let __project: &dyn Fn(&FieldType) -> ret = &(expr);` binding pins the
//! closure's parameter type, so `|m| ...` needs no `: &Type` annotation.
//! The naive `(expr)(&self.x)` codegen would fail E0282 here — this
//! fixture is the regression guard for that.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect(ret = usize, with = |m| m.len())]
    entries: BTreeMap<u64, u32>,
}

fn main() {
    let mut entries = BTreeMap::new();
    entries.insert(1u64, 1u32);
    entries.insert(2u64, 2u32);
    let s = Store { entries };
    assert_eq!(s.inspect_entries(), 2usize);
}
