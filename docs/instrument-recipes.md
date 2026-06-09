# `aristo::instrument` — recipes

Per-pattern cookbook. Each recipe shows the SUT-side code + the harness-side usage. For the underlying rules, see [`instrument-conventions.md`](./instrument-conventions.md).

## Recipe 1 — Snapshot a `SkipMap`'s entries (clone)

SUT side:
```rust
use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct Transaction {
    pub seq: u64,
    pub status: TxStatus,
}

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect)]
    txs: SkipMap<u64, Transaction>,
}
```

Harness side:
```rust
let snap: Vec<(u64, Transaction)> = store.inspect_txs();
// Lean comparison canonicalizes by sorting if needed:
let mut canonical = snap;
canonical.sort_by_key(|(k, _)| *k);
```

The `Vec<(K, V)>` is owned + point-in-time. Mutating the SUT after the call doesn't change `snap`.

## Recipe 2 — Snapshot with a projection type (non-Clone V or canonicalization)

When V contains `Arc<Mutex<_>>`, raw file handles, or other non-Clone internals, OR when the harness needs a subset view for Lean comparison:

SUT side:
```rust
use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

pub struct Transaction {
    pub seq: u64,
    pub status: TxStatus,
    pub locks: std::sync::Arc<parking_lot::Mutex<Vec<LockHandle>>>,  // not Clone
}

pub struct TxnSnapshot {
    pub seq: u64,
    pub status: TxStatus,
    // locks intentionally omitted — harness doesn't need them.
}

impl From<&Transaction> for TxnSnapshot {
    fn from(t: &Transaction) -> Self {
        TxnSnapshot { seq: t.seq, status: t.status }
    }
}

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect(TxnSnapshot))]
    txs: SkipMap<u64, Transaction>,
}
```

Harness side:
```rust
let snap: Vec<(u64, TxnSnapshot)> = store.inspect_txs();
// TxnSnapshot is owned + projected; the Arc<Mutex<_>> internals stay
// behind the safety boundary.
```

## Recipe 3 — Expose an internal constructor for the harness

SUT side:
```rust
use aristo::instrument::expose_pub;

mod buf {
    pub struct Buf { /* ... */ }

    impl Buf {
        #[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub(as = "new_for_test"))]
        pub(crate) fn new(capacity: usize) -> Self { /* ... */ }
    }
}
```

Harness side:
```rust
// The `_for_test` wrapper is pub when the feature is on; calls
// through to the original pub(crate) constructor.
let b = buf::Buf::new_for_test(64);
```

The `_for_test` suffix (Rule 2) makes the boundary visible at every call site.

## Recipe 4 — Expose an internal enum for harness construction

SUT side:
```rust
use aristo::instrument::expose_pub;

#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
pub(crate) enum ParsedOp {
    Get(u64),
    Put(u64, Vec<u8>),
    Delete(u64),
}
```

Harness side:
```rust
// With feature on, ParsedOp is pub + #[doc(hidden)]. Harness
// constructs values and matches variants directly.
let op = ParsedOp::Put(7, vec![1, 2, 3]);
match &op {
    ParsedOp::Put(k, v) => assert_eq!(*k, 7),
    _ => unreachable!(),
}
```

No `as = "..."` is allowed on types (Rule 3); the macro raises visibility in place.

## Recipe 5 — Expose every method in an impl block

SUT side:
```rust
use aristo::instrument::expose_pub;

pub struct Counter { pub n: u64 }

#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
impl Counter {
    pub(crate) fn bump(&mut self) { self.n += 1; }
    pub(crate) fn read(&self) -> u64 { self.n }
    pub(crate) const ZERO: u64 = 0;  // non-fn items left unchanged
}
```

Harness side:
```rust
let mut c = Counter { n: 0 };
c.bump();
assert_eq!(c.read(), 1);
// ZERO stays pub(crate) — only `fn` items are raised.
```

Useful when the SUT has many `pub(crate)` methods in one impl block that the harness needs en masse.

## Recipe 6 — Insert a fault-injection point

SUT side:
```rust
fn write_header(&mut self) -> std::io::Result<()> {
    self.header.version = self.new_version;
    #[cfg(feature = "differential-accessors")]
    aristo::instrument::yield_point!("write_header.before_fsync");
    self.pwrite(&header_bytes, 0)?;
    self.file.sync_all()?;
    Ok(())
}
```

Harness side:
```rust
use aristo::instrument::set_hook;
use std::sync::atomic::{AtomicBool, Ordering};

static INJECTED: AtomicBool = AtomicBool::new(false);

fn fault_at_before_fsync(label: &'static str) {
    if label == "write_header.before_fsync" {
        // Trigger fault behaviour for this scenario.
        INJECTED.store(true, Ordering::Release);
    }
}

#[test]
fn header_write_survives_pre_fsync_crash() {
    set_hook(Some(fault_at_before_fsync));
    // Drive the SUT; INJECTED becomes true when write_header reaches
    // the yield point and the fault is triggered.
    let result = std::panic::catch_unwind(|| run_workload());
    assert!(INJECTED.load(Ordering::Acquire));
    set_hook(None);
}
```

Labels follow the `<fn>.<before|after>_<action>` scheme (Rule 4). One label per call site keeps the harness selector unambiguous.
