//! Clone mode — bare `#[inspect]` on a `SkipMap<K, V>` field emits
//! `inspect_<field>(&self) -> Vec<(K, V)>` by cloning each entry's V.
//! No projection type, no `From` impl, no boilerplate.

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct File {
    pub size: u64,
}

#[derive(Inspect)]
pub struct Cabinet {
    #[inspect]
    files: SkipMap<u64, File>,
}

fn main() {
    let c = Cabinet {
        files: SkipMap::new(),
    };
    c.files.insert(7, File { size: 42 });

    let snap: Vec<(u64, File)> = c.inspect_files();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].0, 7);
    assert_eq!(snap[0].1.size, 42);
}
