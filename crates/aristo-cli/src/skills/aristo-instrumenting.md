---
name: aristo-instrumenting
description: Teaches the coding agent how to apply `aristo::instrument` macros (`Inspect` derive, `expose_pub` attribute, `yield_point!` function-like) to make private state observable to verification harnesses without leaking it into the public API.
sdk_version: {{SDK_VERSION}}
---

# Aristo instrumentation authoring

When the user asks you to expose internal state for a verification or differential-testing harness, or when you proactively decide that an SUT (system-under-test) module needs harness-side observability, follow this guide.

This skill is the **mechanical-layer** counterpart to `aristo-authoring` (which covers the **logical-layer** `#[aristo::intent]` / `#[aristo::assume]` annotations). Both serve verification; the distinction is the kind of claim each makes.

## What instrumentation is

Aristo instrumentation is **codegen** that makes private SUT state observable to a verification harness. Unlike annotations (intent/assume), which are pure compile-time signal, instrumentation produces real runtime accessors / wrappers / hook points that the harness's code references.

Four macros, one surface (`aristo::instrument`):

- **`#[derive(Inspect)]`** — emits a snapshot accessor for any single tagged field. Type-agnostic: bare `#[inspect]` clones a `Clone` field; `#[inspect(ret = T, with = <projector>)]` projects anything else. The harness reads the snapshot to compare SUT state against a model implementation.
- **`#[expose_pub]`** — raises visibility of `pub(crate)` items so the harness can construct them or reference them in signatures. Two flavors: function form (`as = "name_for_test"`) and type/impl form (no rename, just visibility lift).
- **`yield_point!("label")`** — emits a runtime call into a thread-local hook the harness can *observe* (it returns `()`). A passive marker — watch that a code point was reached; it cannot change behaviour.
- **`fault_point!("label")`** — returns a `Decision` (`Continue` / `Inject(u64)`) the SUT branches on to *inject* a fault; the harness installs a capturing policy via `set_fault_hook`. For **interior** faults only — a point inside one operation with no I/O-seam call. Seam-boundary faults (a failing `sync`, a crash that drops bytes) live in the harness's own fault-I/O impl, no macro needed.

All four are feature-gated under `aristo_instrument` and typically wrapped in `#[cfg_attr(feature = "<consumer-alias>", ...)]` so production builds are unaffected.

## When: instrument as the SUT grows, not after

**Add instrumentation in the same diff that makes the underlying SUT state worth observing.** When you write a new SUT field, method, or fault-relevant operation, decide whether the harness needs to observe it; if yes, instrument it then and there. Never sweep instrumentation in at the end.

Why this matters:

- **Harness coverage decays fast.** Adding the field today + thinking "I'll instrument it tomorrow" leaves a hole the harness can't close; the difference between SUT and model is silent until a regression hits production.
- **The fault-injection points are design choices.** Where you place `yield_point!` is a statement about which intermediate states the SUT must be safe in. Choosing those points retroactively forces you to reverse-engineer the safety promise.
- **`expose_pub` placements are visibility decisions.** If a `pub(crate)` constructor needs to be reachable from the harness, decide *now* what its `_for_test` name is, not after you've already written the harness code that references the un-renamed version.

## Before instrumenting: the content gate

Apply this when considering a candidate instrumentation site. The gate filters out instrumentation that adds code-cost without harness value.

1. **Does a verification or differential-testing harness actually need this?** If no harness exists or is planned, don't pre-instrument speculatively. The macros aren't free — every `#[inspect]` field emits an accessor; every `yield_point!` produces a runtime call site.
2. **Is `Inspect` the right shape for it?** `Inspect` is for `&self -> Snapshot` over ONE field with NO parameters — and it covers scalars, `Option`, atomics, and lock-guarded reads via projection (`ret = T, with = |f| …`), so field *shape* is never the blocker. What's out of shape: a **keyed/parameterized** accessor, or a value computed **across several fields**. If an existing `pub(crate)` method already computes it, that's `expose_pub`; otherwise snapshot the field(s) with `Inspect` and do the lookup/computation in the harness (Recipe 14) — don't hand-write a parameterized `inspect_*`. A `_for_test` constructor is `expose_pub`; injecting a fault is `fault_point!`.

Field shape is **never** a blocker. `Inspect` is type-agnostic: bare `#[inspect]` clones any `Clone` field, and `#[inspect(ret = T, with = <projector>)]` projects anything else (non-`Clone`, foreign, or transformed). There is no field shape the macro cannot reach, so **never hand-write an `inspect_*` accessor** — the SDK macros are the only sanctioned instrumentation. If you hit a shape you think the macro can't express, that's a gap to widen aristo for, not to route around.

## The three macros — when each fits

### `#[derive(Inspect)]` for single-field snapshots

Use when the harness needs an owned, point-in-time snapshot of ONE private field. `#[derive(Inspect)]` is **type-agnostic** — it never inspects the field's type. Each `#[inspect]`-tagged field becomes a `pub fn inspect_<field>(&self)` returning an owned snapshot. Pick the mode at the attribute.

**Clone mode** (bare `#[inspect]`) — snapshots any `Clone` field, returning the field's own declared type verbatim:

```rust
use aristo::instrument::Inspect;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Transaction { pub seq: u64, pub status: TxStatus }

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect)]
    txs: BTreeMap<u64, Transaction>,
}
```

Generates `pub fn inspect_txs(&self) -> BTreeMap<u64, Transaction> { self.txs.clone() }`. The `Clone` bound is deferred to rustc, so this works for **any** `Clone` field — scalars, `Option<T>`, `Vec<T>`, `BTreeMap`, `HashMap`, a foreign `SkipMap`, … Add `#[inspect(name = "x")]` to override the method suffix (`inspect_x` instead of `inspect_txs`).

**Projection mode** (`#[inspect(ret = SnapTy, with = <projector>)]`) — snapshots everything else: non-`Clone` fields, or when the harness wants a canonical / filtered / fanned-out view. `with` is any expression callable as `Fn(&FieldType) -> SnapTy`:

```rust
use aristo::instrument::Inspect;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Inspect)]
pub struct Clock {
    // non-Clone field: load the atomic into an owned snapshot.
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = u64, with = |a| a.load(Ordering::Acquire))
    )]
    ticks: AtomicU64,
}
```

`with` is a path to a named free function (best for reuse / complex bodies) OR an inline closure (best for one-liners). The closure needs **no** parameter annotation — the macro pins the type for you. `ret` is **required** and echoed verbatim as the return type: a syntactic proc-macro cannot infer a closure's return type, so the ascription is mandatory. `name = "x"` may be added in any order.

Use the named-function form when the projection is non-trivial or shared:

```rust
#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(
        feature = "differential-accessors",
        inspect(ret = Vec<(u64, TxnSnapshot)>, with = project_txs)
    )]
    txs: SkipMap<u64, Transaction>, // foreign type — no trait impl required
}

fn project_txs(txs: &SkipMap<u64, Transaction>) -> Vec<(u64, TxnSnapshot)> {
    txs.iter().map(|e| (*e.key(), TxnSnapshot::from(e.value()))).collect()
}
```

Two properties make projection mode the tool of choice for foreign / concurrent fields:

- **Orphan-safe for any foreign field type.** The macro emits an *inherent* method on your struct and names the projector as an arbitrary expression, so a foreign field type (crossbeam's `SkipMap`, etc.) is never the `Self` of a foreign trait. You **never** need `impl SomeTrait for SkipMap` or `impl From<&SkipMap> for …` — the projector is just a function/closure taking `&Field`.
- **Strictly more expressive than a per-entry mapping.** Because the projector sees the WHOLE field, it can FILTER (keep only some entries) and FAN-OUT (turn one map entry into N snapshot rows) — things a per-entry `From<&V>` could not do.

Reach for projection mode for any non-`Clone` or transformed field: atomics (`|a| a.load(Ordering::Acquire)`), lock-guarded state (`|l| l.read().map(|g| g.clone())`), `Option<Foreign>` → `Option<Primitive>`, a filtered subset of a map, a crate-private enum's variant tag (`|c| match c { A(_) => "a", B(_) => "b" }` — observe representation without leaking the type; see Recipe 10), and so on.

Untagged fields are ignored (no codegen). And remember the taxonomy: `Inspect` is for `&self -> Snapshot` over ONE field with NO parameters — anything parameterized or computed from multiple fields is `expose_pub` (below), not `Inspect`.

### `#[expose_pub]` for visibility raising

Use when the harness needs to call a `pub(crate)` function, construct a `pub(crate)` type, or reference items inside a `pub(crate)` impl block.

**Function form** — requires `as = "<wrapper_name>"`:

```rust
impl Buf {
    #[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub(as = "new_for_test"))]
    pub(crate) fn new(capacity: usize) -> Self { /* ... */ }
}
```

Generates a `pub fn new_for_test(capacity: usize) -> Self { Self::new(capacity) }` wrapper alongside the original. The `_for_test` suffix (convention) makes every harness call site visible to `grep`.

**Type form** — no `as = "..."`; the macro raises the existing visibility in place:

```rust
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
pub(crate) enum ParsedOp { Get(u64), Put(u64, Vec<u8>) }
```

Same name, lifted visibility, `#[doc(hidden)]` added so the public-rustdoc surface stays clean.

**Impl-block form** — raises visibility on every method inside, in place. Associated consts / types are left unchanged:

```rust
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub)]
impl Counter {
    pub(crate) fn bump(&mut self) { /* ... */ }
    pub(crate) fn read(&self) -> u64 { /* ... */ }
}
```

Useful when an entire impl block needs harness access; avoids per-method `#[expose_pub]` boilerplate.

### `yield_point!` for observation markers

Use at SUT code locations a harness wants to *observe* — confirm a code point was reached, count visits. It returns `()`, so it cannot change behaviour; to make a fault actually happen at a point, use `fault_point!` (below), not `yield_point!`:

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

The harness installs a callback via `aristo::instrument::set_hook(Some(my_callback))`; when `write_header` reaches the labelled point, the callback fires with the label string.

Labels follow the `<fn>.<before|after>_<action>` scheme. One label per call site within a function; the harness selects which point to inject by string match.

### `fault_point!` for injecting interior faults

Use when the harness must *inject* a fault at a point **inside one operation that has no I/O-seam call** — a non-I/O failure (allocation, checksum, in-memory rebuild) or a crash mid-construction of a single write. `fault_point!("label")` returns a `Decision`; the SUT branches on it; the harness installs a capturing policy:

```rust
use aristo::instrument::{fault_point, Decision};

fn rebuild_index(&mut self) -> Result<(), Corrupt> {
    for entry in self.scan() {
        #[cfg(feature = "fault-injection")]
        if let Decision::Inject(_) = fault_point!("index.rebuild.per_entry") {
            return Err(Corrupt);   // the SUT decides what "fail" means
        }
        self.insert(entry);
    }
    Ok(())
}
// harness: set_fault_hook(Some(Box::new(move |l| { /* count, return Inject on the Nth */ })))
```

aristo carries only *when* (the label, plus an opaque `u64` for parameterized faults — an errno, a short-write length); the SUT owns *what*. **Decide observe vs inject vs seam-fault before reaching for it:** if you only need to watch a point, that's `yield_point!`; if the fault lands on a seam call (a failing `sync`/`append`, a crash that drops bytes), it lives in the harness's own fault-I/O impl (no macro) — `fault_point!` is strictly for interior faults. See `docs/instrument-recipes.md` Recipe 13 (and Recipe 12 for the seam).

## Convention rules (quick reference)

Full discussion in `docs/instrument-conventions.md`. Summary:

1. **Pick clone or projection per the field.** Bare `#[inspect]` clones any `Clone` field; `#[inspect(ret = T, with = <projector>)]` projects everything else (non-`Clone`, foreign, or transformed).
2. **Name `expose_pub` function wrappers with a `_for_test` suffix.** `grep _for_test` finds every harness leak.
3. **Don't rename types** — `expose_pub` on enum/struct/type/impl raises visibility in place; `as = "..."` is forbidden and rejected with an error.
4. **Label `yield_point!` calls with `<fn>.before_<action>` / `<fn>.after_<action>`.** Consistent labels keep harness selectors stable across SUT changes.
5. **Gate macro invocations with `cfg_attr`.** Keeps production builds at zero residual cost. Consumers alias their preferred flag name onto `aristo_instrument`.

## Common pitfalls

### Forgetting the `cfg_attr` gate

```rust
// WRONG — forces every consumer to enable aristo_instrument:
#[derive(aristo::instrument::Inspect)]
pub struct MvStore { ... }

// RIGHT — the macro only applies when the consumer's feature is on:
#[cfg_attr(feature = "differential-accessors", derive(aristo::instrument::Inspect))]
pub struct MvStore { ... }
```

Without the gate, every consumer compiles with the instrument surface active, defeating the opt-in design.

### Using clone mode on a non-`Clone` field

If a field holds an `AtomicU64`, `Arc<Mutex<_>>`, a raw `File`, or any other non-`Clone` type, bare `#[inspect]` (clone mode) fails to compile (rustc rejects the deferred `Clone` bound). The error points at the **offending field** (`the trait bound `Foo: Clone` is not satisfied`), so you can see exactly which one to fix. Switch that field to projection mode — `#[inspect(ret = T, with = <projector>)]` — with a projector that reads out just the owned data you need (`|a| a.load(Ordering::Acquire)`, `|l| l.lock().unwrap().clone()`, …). No `From` impl and no trait on the field type are required.

One trap with the rustc message: for a **`Clone` container of a non-`Clone` element** (e.g. `BTreeMap<u16, PrivateEnum>`), rustc blames the inner type and suggests `consider annotating `PrivateEnum` with #[derive(Clone)]`. **Ignore that suggestion** — the element is SUT code the test boundary forbids you to edit. Project the *field* instead (`with = |m| m.iter().map(|(k, v)| (k.clone(), tag(v))).collect()`), reading the element down to a harness-nameable shape inside the closure.

### Cloning a field whose type is crate-private

Clone mode returns the field's declared type verbatim, so a `pub` accessor can leak a crate-private type. The snapshot compiles, but the harness crate can't fully use it: it reads `pub` primitive fields yet cannot name the type for an annotation or `match` a private enum variant (`error[E0603]`). Project to a harness-nameable type — the `with` closure runs inside the SUT where the private type is in scope, and only the public `ret` crosses the boundary:

```rust
// Entry / Tag are crate-private; project to primitives instead of cloning:
#[cfg_attr(feature = "...", inspect(
    ret = Vec<(Vec<u8>, u64, u32, bool)>,
    with = |kd| kd.iter().map(|(k, e)| (k.clone(), e.offset, e.len, matches!(e.tag, Tag::Tombstone))).collect()
))]
keydir: BTreeMap<Vec<u8>, Entry>,
```

### Projector visibility across modules

A `with =` projector is named from the **tagged struct's module** — that's where the derive emits the `inspect_*` method. If the projector lives in a *different* module (e.g. a child `differential` submodule), it must be reachable from the struct's module, and if its signature names a **module-private value type**, prefer `pub(super)` over `pub(crate)`:

```rust
// In a child `differential` module, projecting a field whose value type
// (`TransactionState`) is private to the parent module:
pub(super) fn project_finalized(
    m: &SkipMap<TxID, TransactionState>,
) -> Vec<(TxID, FinalStateSnapshot)> {
    m.iter().map(|e| (*e.key(), FinalStateSnapshot::from(e.value()))).collect()
}
```

`pub(crate)` would compile but trips rustc's `private_interfaces` lint — it exposes the private value type more widely than the projector needs. `pub(super)` is exactly enough to be named from the parent module's derive. (For a projector in the *same* module as the struct, a bare `fn` is fine.)

### Reaching an inner type's private fields (layered derive)

A `with =` projector over `Vec<Inner>` / `BTreeMap<K, Inner>` can only see `Inner`'s **public** API — it's type-checked in the outer struct's module, so it can't read fields private to `Inner`'s own module (`error: field … is private`). Don't reach for `expose_pub` or hand-write an accessor. Put a second `#[derive(Inspect)]` on `Inner` that projects the private field **inside `Inner`'s module**; the generated `inspect_*` is `pub`, so the outer projector composes it:

```rust
// inner: projects its own private fields, in its own module
#[cfg_attr(feature = "...", derive(aristo::instrument::Inspect))]
pub struct SsTable {
    #[cfg_attr(feature = "...", inspect(ret = Vec<(Vec<u8>, u32)>, with = |ms| ms.iter().map(|m| (m.first_key.clone(), m.len)).collect()))]
    metas: Vec<BlockMeta>,   // private here
}
// outer: composes inner.inspect_metas() across the collection
inspect(ret = Vec<(usize, Vec<u8>, u32)>, with = |ssts| ssts.iter().enumerate()
    .flat_map(|(i, s)| s.inspect_metas().into_iter().map(move |(k, n)| (i, k, n))).collect())
```

See `docs/instrument-recipes.md` Recipe 9. The one genuine wall: a *foreign / sealed* inner type you can't annotate exposes only its public API — rare, and not something to route around with `unsafe`.

### An item trapped in a private module

`expose_pub` raises an item's visibility but can't open the module around it. A `pub` item inside a private `mod` (`mod record; pub fn parse_record …`) still isn't reachable — `error[E0603]: module `record` is private`. Don't change `mod record` to `pub mod record` (that edits the SUT's structure). Re-export the item at the crate root instead, feature-gated and doc-hidden:

```rust
#[cfg(feature = "...")]   // #[cfg], not #[cfg_attr] — a bare private `use` would warn when off
#[doc(hidden)]
pub use crate::record::parse_record;
```

A `pub(crate)` target needs the two-step: `expose_pub` it to `pub` first, then re-export. See `docs/instrument-recipes.md` Recipe 11. One gotcha: re-exporting can wake public-API clippy lints that were dormant while the item was private (e.g. `len_without_is_empty` on a `len`-bearing trait) — silence them with a `#[cfg_attr(feature = "...", allow(...))]` gated to the same feature, so production stays clean.

### `yield_point!` on a pure (non-I/O) data structure

`yield_point!` is for *pausing / failing* at an I/O or fault boundary. A pure in-memory structure has nothing to fail, so a yield point there only announces an event — and the `&'static str` label can't carry *which* item or *how big*. To observe pure state (a representation switch, a cardinality threshold), use an `Inspect` projection: `inspect_*()` returns typed, owned data the harness asserts on directly (e.g. `(high, "array"/"bitset", cardinality)`), with no global `static` to route observations through. Reserve `yield_point!` for genuine I/O / fault seams.

### Renaming a type with `as = "..."`

```rust
// WRONG — types FORBID `as = "..."`:
#[expose_pub(as = "ParsedOpForTest")]
pub(crate) enum ParsedOp { ... }
// error: `#[expose_pub]` on a type / impl-block does not accept arguments
```

Renaming a type would cascade through every reference. The macro raises visibility on the SAME name; consumers gate with `cfg_attr` to keep the original visibility off-feature.

### `yield_point!` in `const fn` context

The runtime hook (`__yield_point`) isn't callable from const context. Putting `yield_point!` inside a `const fn` produces a confusing rustc error. Avoid; the macro is for runtime hook injection only.

### Vague labels

```rust
// WRONG — "checkpoint" and "done" don't tell the harness what they mark:
yield_point!("checkpoint");
yield_point!("done");

// RIGHT — labels carry fn name + side + action:
yield_point!("commit.before_log_sync");
yield_point!("commit.after_log_sync");
```

The harness selects yield points by string match. Vague labels make harness code ambiguous and fragile.

## Working pattern: instrument inline

When you write SUT code that the harness needs to observe, apply the relevant macro **in the same diff** as the underlying code. The mental sequence:

1. **Add the SUT field / method / fault-relevant operation.** Write the production code first.
2. **Ask the content-gate questions.** Does the harness need to see this? Is the macro the right shape?
3. **If yes**: apply the macro, with `cfg_attr` gating the consumer's preferred feature flag.
4. **If no**: skip the macro. Don't instrument state no harness reads — but if a harness does read it, instrument it with the macro; never hand-write an `inspect_*` accessor as a one-off.

End-of-diff sweeps systematically miss fault-injection points (because they're not visible in the production code shape) and produce inconsistent label / wrapper naming. Instrument inline, with the same rigor you apply to writing intents.

## How to use this skill operationally

When the user says "expose this for testing", "the harness needs to read X", "add a fault-injection point at Y":

1. Identify which macro fits (Inspect / expose_pub / yield_point!).
2. Apply the relevant convention rule (clone vs project; `_for_test` naming; label scheme).
3. Wrap the macro invocation in `#[cfg_attr(feature = "<consumer-alias>", ...)]`.
4. If introducing `yield_point!`, also write or update the harness-side `set_hook` callback.
5. Verify with `cargo check --features <consumer-alias>` that the SUT compiles with instrumentation on and `cargo check` (no features) that it still compiles without.
