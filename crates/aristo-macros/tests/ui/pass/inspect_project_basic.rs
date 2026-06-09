//! Project mode — `#[inspect(T)]` on a `SkipMap<K, V>` field emits
//! `inspect_<field>(&self) -> Vec<(K, T)>` by applying the user's
//! `impl From<&V> for T` per entry. Used when the snapshot should
//! differ from V (e.g., hide internal fields, canonicalize for Lean
//! comparison, drop non-Clone internals).

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

pub struct File {
    pub size: u64,
    pub _internal_handle: u32,
}

pub struct FileView {
    pub size: u64, // FileView intentionally omits `_internal_handle`.
}

impl From<&File> for FileView {
    fn from(f: &File) -> Self {
        FileView { size: f.size }
    }
}

#[derive(Inspect)]
pub struct Cabinet {
    #[inspect(FileView)]
    files: SkipMap<u64, File>,
}

fn main() {
    let c = Cabinet {
        files: SkipMap::new(),
    };
    c.files.insert(
        7,
        File {
            size: 42,
            _internal_handle: 999,
        },
    );

    let snap: Vec<(u64, FileView)> = c.inspect_files();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].0, 7);
    assert_eq!(snap[0].1.size, 42);
}
