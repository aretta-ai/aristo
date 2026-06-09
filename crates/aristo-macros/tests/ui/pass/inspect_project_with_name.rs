//! Project mode + `name = "<suffix>"` override. Both forms compose:
//! the positional `T` selects project mode; `name = "..."` renames the
//! method. Argument order does not matter.

use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

pub struct Transaction {
    pub seq: u64,
    pub _arc_internal: u32,
}

pub struct TxnView {
    pub seq: u64,
}

impl From<&Transaction> for TxnView {
    fn from(t: &Transaction) -> Self {
        TxnView { seq: t.seq }
    }
}

#[derive(Inspect)]
pub struct Store {
    #[inspect(TxnView, name = "txs")]
    archived_transactions: SkipMap<u64, Transaction>,
}

fn main() {
    let s = Store {
        archived_transactions: SkipMap::new(),
    };
    s.archived_transactions.insert(
        1,
        Transaction {
            seq: 7,
            _arc_internal: 999,
        },
    );

    // Accessor is `inspect_txs`, returning `Vec<(u64, TxnView)>`.
    let snap: Vec<(u64, TxnView)> = s.inspect_txs();
    assert_eq!(snap[0].1.seq, 7);
}
