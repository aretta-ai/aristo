//! Projection mode is incomplete with only `ret =`: a return type without
//! a projector cannot produce a value. The macro errors, pointing at the
//! missing `with =`.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
struct Store {
    #[inspect(ret = usize)]
    entries: BTreeMap<u64, u32>,
}

fn main() {}
