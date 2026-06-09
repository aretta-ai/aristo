//! v0.2.6 regression case — `Inspect` derive on a struct with generic
//! parameters and trait bounds.
//!
//! v0.2.5 shipped the derive without threading the struct's generics
//! through the emitted impl header, producing `impl MvStore { ... }`
//! when the struct was `MvStore<Clock: LogicalClock>` — E0107
//! "missing generics for struct". This case mirrors the failure shape
//! from the Turso fork's `MvStore<Clock>` and locks the fix in place
//! so future shapes (more type params, lifetime params, where clauses)
//! don't silently regress.

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

trait LogicalClock {}

struct DefaultClock;
impl LogicalClock for DefaultClock {}

#[derive(Clone)]
struct Tx {
    id: u64,
}

struct TxSnap {
    id: u64,
}

impl From<&Tx> for TxSnap {
    fn from(t: &Tx) -> Self {
        TxSnap { id: t.id }
    }
}

#[derive(Inspect)]
struct Store<Clock: LogicalClock> {
    #[inspect(TxSnap)]
    txs: SkipMap<u64, Tx>,
    _clock: std::marker::PhantomData<Clock>,
}

fn main() {
    let s = Store::<DefaultClock> {
        txs: SkipMap::new(),
        _clock: Default::default(),
    };
    s.txs.insert(7, Tx { id: 42 });
    let snap: Vec<(u64, TxSnap)> = s.inspect_txs();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].0, 7);
    assert_eq!(snap[0].1.id, 42);
}
