# `aristo::instrument` — recipes

Per-pattern cookbook. Each recipe shows the SUT-side code + the harness-side usage. For the underlying rules, see [`instrument-conventions.md`](./instrument-conventions.md).

## Recipe 1 — Snapshot a foreign / concurrent collection (named projector)

The canonical way to snapshot a concurrent or foreign-typed field (crossbeam's `SkipMap`, a third-party map, etc.) is projection mode with a **named** free-function projector. The projector takes `&Field` and returns the owned snapshot; the macro emits an *inherent* method on your struct, so the foreign type is never the `Self` of a foreign trait — no orphan-rule trouble, no `impl` on the field type, no `From<&V>`.

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

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<(u64, TxnSnapshot)>, with = project_txs)
    )]
    txs: SkipMap<u64, Transaction>,
}

fn project_txs(txs: &SkipMap<u64, Transaction>) -> Vec<(u64, TxnSnapshot)> {
    txs.iter()
        .map(|e| (*e.key(), TxnSnapshot { seq: e.value().seq, status: e.value().status }))
        .collect()
}
```

Harness side:
```rust
let snap: Vec<(u64, TxnSnapshot)> = store.inspect_txs();
// Owned + point-in-time; the Arc<Mutex<_>> internals stay behind the
// safety boundary. Sort by key before a Lean comparison if needed.
```

`ret` is required (a proc-macro cannot infer the return type) and is echoed verbatim. Because the projector sees the WHOLE field it can also FILTER (drop entries) and FAN-OUT (emit N rows from one entry) — strictly more than a per-entry mapping could express:

```rust
fn project_recovered(rows: &SkipMap<i64, Vec<u32>>) -> Vec<(i64, u32)> {
    rows.iter()
        .filter(|e| *e.key() < 0)                                  // keep a subset
        .flat_map(|e| e.value().clone().into_iter().map(move |v| (*e.key(), v)))  // one entry → N rows
        .collect()
}
```

## Recipe 2 — Snapshot a `Clone` field directly (bare `#[inspect]`)

When the field is already `Clone` and the harness wants the full data, bare `#[inspect]` clones it and returns the field's own declared type — no projector, no snapshot type.

SUT side:
```rust
use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Transaction {
    pub seq: u64,
    pub status: TxStatus,
}

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect)]
    txs: BTreeMap<u64, Transaction>,
}
```

Generates `pub fn inspect_txs(&self) -> BTreeMap<u64, Transaction> { self.txs.clone() }`. The `Clone` bound is deferred to rustc, so this works for any `Clone` field — scalars, `Option<T>`, `Vec<T>`, `HashMap`, `BTreeMap`, … Add `#[inspect(name = "x")]` to override the method suffix.

Harness side:
```rust
let snap: BTreeMap<u64, Transaction> = store.inspect_txs();
// Owned + point-in-time. Mutating the SUT after the call doesn't change snap.
```

## Recipe 3 — Snapshot a derived view (inline closure)

For a one-line projection, skip the named function and inline a closure in `with`. The macro pins the closure's parameter type, so it needs NO `: &Type` annotation; `ret` still spells the return type.

SUT side:
```rust
use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Inspect)]
pub struct Index {
    // Snapshot just the keys, in order — not the whole map.
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<u64>, with = |m| m.keys().copied().collect())
    )]
    entries: BTreeMap<u64, Record>,
}
```

Harness side:
```rust
let ids: Vec<u64> = index.inspect_entries();
```

Reach for the named-function form (Recipe 1) when the body is non-trivial or shared across fields; the inline closure is for genuinely one-line views.

## Recipe 4 — Snapshot a non-`Clone` field (atomic, lock-guarded)

Projection mode is the tool for fields that aren't `Clone`: the projector reads out an owned value. No `From` impl, no trait on the field type.

Atomic — load into an owned scalar:
```rust
use aristo::instrument::Inspect;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Inspect)]
pub struct Clock {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = u64, with = |a| a.load(Ordering::Acquire))
    )]
    ticks: AtomicU64,
}
```

Lock-guarded state — acquire, read, and project the guarded data into an owned snapshot (never hand a guard across the boundary):
```rust
use aristo::instrument::Inspect;
use std::sync::RwLock;

#[derive(Inspect)]
pub struct Catalog {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<String>, with = |l| l.read().unwrap().clone())
    )]
    names: RwLock<Vec<String>>,
}
```

Harness side:
```rust
let now: u64 = clock.inspect_ticks();
let names: Vec<String> = catalog.inspect_names();
```

The snapshot is owned and point-in-time; the atomic / lock stays inside the SUT.

## Recipe 5 — Expose an internal constructor for the harness

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

## Recipe 6 — Expose an internal enum for harness construction

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

## Recipe 7 — Expose every method in an impl block

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

## Recipe 8 — Insert a fault-injection point

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

## Recipe 9 — Reach an inner type's private fields (layered derive)

When an outer projector ranges over a collection of an inner struct — `Vec<Inner>`, `BTreeMap<K, Inner>` — and needs fields that are **private to the inner type's own module**, the outer projector can't read them: a `with` projector is type-checked with the visibility of the module where its `#[derive(Inspect)]` is invoked, so a projector in `db` cannot name `SsTable`'s private fields. Put a *second* `#[derive(Inspect)]` on the inner type, projecting its private fields **inside the inner module** (where they're visible); the generated `inspect_*` is `pub`, so the outer projector composes it across the collection.

SUT side:
```rust
// in `mod sstable` — BlockMeta and SsTable.metas are private here
#[cfg_attr(feature = "differential-accessors", derive(aristo::instrument::Inspect))]
pub struct SsTable {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<(Vec<u8>, Vec<u8>, u32)>,
                with = |ms| ms.iter().map(|m| (m.first_key.clone(), m.last_key.clone(), m.len)).collect())
    )]
    metas: Vec<BlockMeta>,  // private to this module
}

// in `mod db` — composes the inner accessor across the collection
#[cfg_attr(feature = "differential-accessors", derive(aristo::instrument::Inspect))]
pub struct Db {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<(usize, Vec<u8>, Vec<u8>, u32)>,
                with = |ssts| ssts.iter().enumerate()
                    .flat_map(|(i, s)| s.inspect_metas().into_iter().map(move |(f, l, n)| (i, f, l, n)))
                    .collect())
    )]
    ssts: Vec<SsTable>,
}
```

Harness side:
```rust
let rows: Vec<(usize, Vec<u8>, Vec<u8>, u32)> = db.inspect_ssts();
// (sst_index, first_key, last_key, block_len), flattened across every SST.
```

The inner derive's `inspect_metas()` does the private read inside `sstable`; the outer projector only ever touches the inner type's **public** `inspect_*` surface. **Limit:** this works because you can annotate the inner type. If the inner type is foreign or sealed — you can't add `#[derive(Inspect)]` to it — only its public API is reachable. That is a genuine Rust visibility wall, not an aristo gap; it is rare, and the answer is not `unsafe` or a hand-written accessor.

## Recipe 10 — Observe a private enum's representation (projection-to-tag)

To *observe* which variant a crate-private enum is in — a representation invariant, a state-machine phase — project it to a stable tag (a `&'static str`, or a small plain enum) inside the SUT. The `with` closure runs in the enum's own module, so it can `match` private variants; only the tag crosses the boundary, and the enum stays fully private. Contrast Recipe 6, which raises the *whole* enum to `pub` + `#[doc(hidden)]` for harness **construction** — observation never needs that.

SUT side:
```rust
// `mod container` — Container is private and non-`Clone`
pub enum Container { Array(Vec<u16>), Bitset(Box<[u64; 1024]>) }

#[cfg_attr(feature = "differential-accessors", derive(aristo::instrument::Inspect))]
pub struct Bitmap {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<(u16, &'static str, u32)>,
                with = |cs| cs.iter().map(|(&hi, c)| {
                    let kind = match c { Container::Array(_) => "array", Container::Bitset(_) => "bitset" };
                    (hi, kind, c.len() as u32)
                }).collect())
    )]
    containers: BTreeMap<u16, Container>,
}
```

Harness side:
```rust
// Assert the array→bitset switch happened at the right cardinality —
// without ever naming Container outside its crate.
let repr: Vec<(u16, &'static str, u32)> = bitmap.inspect_containers();
assert_eq!(repr, vec![(0u16, "bitset", 4097)]);
```

Use projection-to-tag to **observe** representation; reach for `expose_pub` (Recipe 6) only when the harness must **construct** the enum.

## Recipe 11 — Re-export an item trapped in a private module

`expose_pub` raises an item's *visibility*, but it can't open the *module* around it. A common SUT shape is a private module that re-exports a few names (`mod record; … pub use record::Lsn;`). An item that's already `pub` *inside* such a module still isn't reachable from the harness — `error[E0603]: module `record` is private`. Add a feature-gated, doc-hidden re-export at the crate root, exactly like the crate's own public API but `instr`-gated:

```rust
// lib.rs, at the crate root
#[cfg(feature = "differential-accessors")]
#[doc(hidden)]
pub use crate::record::parse_record;
```

The harness now names `yourcrate::parse_record`; the module stays private, and `#[doc(hidden)]` keeps it out of public rustdoc. Use `#[cfg(...)]`, **not** `#[cfg_attr(..., expose_pub)]`: a re-export this simple needs no macro, and a bare private `use` left behind when the feature is off would trip `unused_imports` under `-D warnings`.

If the target is `pub(crate)` rather than `pub`, a re-export alone fails (`error[E0364]: … cannot be re-exported`). Raise the item with `expose_pub` first, then re-export — both gated on the same feature:

```rust
// in `mod record`
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
pub(crate) struct Frame { /* … */ }   // -> pub + #[doc(hidden)] when the feature is on

// at the crate root
#[cfg(feature = "differential-accessors")]
#[doc(hidden)]
pub use crate::record::Frame;
```

**Heads-up — a re-export can wake dormant public-API lints.** While the item was module-private, clippy lints that only apply to *public* API stayed quiet. Re-exporting makes it public API (under the feature), so those lints fire — e.g. a trait with a `len` method but no `is_empty` trips `clippy::len_without_is_empty`. Silence it with a **feature-gated** allow on the item, gated to the *same* feature as the re-export, so production (feature off) is untouched and the lint is only suppressed while it's awake:

```rust
// in `mod io` — only public (and only lint-checked) under the feature
#[cfg_attr(feature = "differential-accessors", allow(clippy::len_without_is_empty))]
pub trait BlockIo {
    fn len(&self) -> u64;
    // append, sync, …
}
```

## Recipe 12 — Simulate a crash (SUT-side I/O seam)

Crash-durability — *acknowledged data survives a crash; never-fsync'd data may not* — is the one property aristo's macros can't reach. Testing it means **dropping the bytes that were never fsync'd**, which requires substituting the engine's I/O underneath it. A fake disk has to implement *your* I/O contract, so the seam is SUT-specific and lives in the SUT, not in an aristo macro. This is the single spec class where the harness contact surface legitimately exceeds annotations — and it's clean dependency injection, not a hack.

1. Route I/O through a trait the SUT owns, stored as a trait object:
```rust
pub trait BlockIo {
    fn append(&mut self, bytes: &[u8]) -> std::io::Result<u64>;
    fn sync(&mut self) -> std::io::Result<()>;
    // read_at, len, …
}

pub struct Db { io: Box<dyn BlockIo>, /* … */ }
```
2. Keep the production constructor hard-coding real I/O; add a **test-only injecting** constructor (feature- or `cfg(test)`-gated):
```rust
impl Db {
    pub fn open(dir: &Path) -> std::io::Result<Self> { Self::with_io(dir, Box::new(StdIo::open(dir)?)) }

    #[cfg(any(test, feature = "fault-injection"))]
    pub fn open_with_io(dir: &Path, io: Box<dyn BlockIo>) -> std::io::Result<Self> { Self::with_io(dir, io) }
}
```
3. Write a fault-injecting `BlockIo` that models a crash — never-synced appends live in a buffer a modeled crash discards; only `sync()` makes them durable, and you can fail the *N*th `sync`:
```rust
struct FaultyIo { durable: Vec<u8>, pending: Vec<u8>, fail_sync_after: Option<usize> }
// crash() drops `pending`; sync() moves `pending` into `durable` (or errors on the Nth call).
```
4. Drive it from the harness: build the `Db` with `FaultyIo`, run the workload, trigger the crash, reopen against the durable bytes, and assert acknowledged data survived while un-synced data is gone.

Notice what is **not** here: no aristo macro fires for the headline faults. The crash (`crash()` discards `pending`) and "fail the Nth sync" (a counter in `FaultyIo::sync` returning `Err`) both live in your own fault I/O — the harness owns the failing function, because each fault coincides with a seam call it already controls. Aristo's macros cover everything *around* the seam (`Inspect` to snapshot recovered state, `expose_pub` for a `_for_test` constructor); the only fault that needs a macro is an *interior* one — a point inside one operation with no seam call to attach to (Recipe 13). The I/O trait itself is yours to design.

## Recipe 13 — Inject an interior fault (`fault_point!`)

Recipe 12's faults all land on a seam call — `sync`, `append`, `crash` — so the harness expresses them inside its own `FaultyIo`, no aristo macro needed. `fault_point!` is for the residual case: a fault at a point *inside* one operation with **no seam call to grab** — a non-I/O failure (an allocation, a checksum verify, an in-memory rebuild) or a crash mid-construction of a single write, before the bytes ever reach the seam.

`fault_point!("label")` returns a `Decision` the SUT branches on; the harness installs a capturing policy via `set_fault_hook` (the counter lives in the closure — no global static).

SUT side — an interior fault with no I/O call at the point:
```rust
use aristo::instrument::{fault_point, Decision};

fn rebuild_index(&mut self) -> Result<(), Corrupt> {
    for entry in self.scan() {
        // Pure in-memory work — nothing calls the I/O seam here, so there is
        // no seam method for the harness to fail. Expose an explicit handle:
        #[cfg(feature = "fault-injection")]
        if let Decision::Inject(_) = fault_point!("index.rebuild.per_entry") {
            return Err(Corrupt);   // the SUT decides what "fail" means
        }
        self.insert(entry);
    }
    Ok(())
}
```

Harness side — fail the 3rd entry, counter captured in the closure:
```rust
use aristo::instrument::{set_fault_hook, Decision};

let mut n = 0;
set_fault_hook(Some(Box::new(move |label| {
    if label == "index.rebuild.per_entry" {
        n += 1;
        if n == 3 { return Decision::Inject(0); }
    }
    Decision::Continue
})));
// drive the rebuild; assert it fails cleanly on the 3rd entry and recovers.
set_fault_hook(None);
```

The opaque `u64` in `Inject(u64)` is a harness→SUT channel for *parameterized* faults — a short-write prefix length, an errno, a timeout — that aristo never interprets; use a plain `Inject(_)` when the site has a single effect.

**When NOT to use it:** if the fault coincides with a seam call (a failing `sync`/`append`, a crash that drops buffered bytes), put it in your `FaultyIo` (Recipe 12) — `fault_point!` would just be a less-direct way to reach the same seam. As you decompose the seam finer, more "interior" faults become seam-boundary; `fault_point!`'s domain is the faults that stay strictly inside one un-decomposed operation.
