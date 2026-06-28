//! The pre-0.3.0 positional projection form `#[inspect(T)]` was removed.
//! It now errors with a message pointing at the `ret =`/`with =`
//! replacement, so consumers migrating off the old SkipMap-only surface
//! get a clear diagnostic rather than silent misbehavior.

use aristo::instrument::Inspect;
use std::collections::BTreeMap;

struct FileView;

#[derive(Inspect)]
struct Store {
    #[inspect(FileView)]
    entries: BTreeMap<u64, u32>,
}

fn main() {}
