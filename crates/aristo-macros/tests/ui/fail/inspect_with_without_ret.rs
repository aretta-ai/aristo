//! Projection mode is incomplete with only `with =`: a syntactic
//! proc-macro cannot infer a closure's return type, so `ret =` is
//! mandatory. The macro errors, pointing at the missing `ret =`.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect(with = |m| m.len())]
    entries: BTreeMap<u64, u32>,
}

fn main() {}
