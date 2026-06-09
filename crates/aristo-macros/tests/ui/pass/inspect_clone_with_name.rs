//! Clone mode + `name = "<suffix>"` override. The accessor is
//! renamed to `inspect_<suffix>` (the `inspect_` prefix is automatic).

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct State {
    pub epoch: u64,
}

#[derive(Inspect)]
pub struct Store {
    #[inspect(name = "states")]
    archived_states: SkipMap<u64, State>,
}

fn main() {
    let s = Store {
        archived_states: SkipMap::new(),
    };
    s.archived_states.insert(1, State { epoch: 99 });

    // Accessor is `inspect_states`, not `inspect_archived_states`.
    let snap: Vec<(u64, State)> = s.inspect_states();
    assert_eq!(snap[0].1.epoch, 99);
}
