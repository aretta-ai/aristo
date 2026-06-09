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

Three macros, one surface (`aristo::instrument`):

- **`#[derive(Inspect)]`** — emits snapshot accessors over `SkipMap<K, V>` fields. The harness reads the snapshot to compare SUT state against a model implementation.
- **`#[expose_pub]`** — raises visibility of `pub(crate)` items so the harness can construct them or reference them in signatures. Two flavors: function form (`as = "name_for_test"`) and type/impl form (no rename, just visibility lift).
- **`yield_point!("label")`** — emits a runtime call into a thread-local hook that the harness uses for fault injection (pause / fail / re-order at controlled call sites).

All three are feature-gated under `aristo_instrument` and typically wrapped in `#[cfg_attr(feature = "<consumer-alias>", ...)]` so production builds are unaffected.

## When: instrument as the SUT grows, not after

**Add instrumentation in the same diff that makes the underlying SUT state worth observing.** When you write a new SUT field, method, or fault-relevant operation, decide whether the harness needs to observe it; if yes, instrument it then and there. Never sweep instrumentation in at the end.

Why this matters:

- **Harness coverage decays fast.** Adding the field today + thinking "I'll instrument it tomorrow" leaves a hole the harness can't close; the difference between SUT and model is silent until a regression hits production.
- **The fault-injection points are design choices.** Where you place `yield_point!` is a statement about which intermediate states the SUT must be safe in. Choosing those points retroactively forces you to reverse-engineer the safety promise.
- **`expose_pub` placements are visibility decisions.** If a `pub(crate)` constructor needs to be reachable from the harness, decide *now* what its `_for_test` name is, not after you've already written the harness code that references the un-renamed version.

## Before instrumenting: the content gate

Apply this when considering a candidate instrumentation site. The gate filters out instrumentation that adds code-cost without harness value.

1. **Does a verification or differential-testing harness actually need this?** If no harness exists or is planned, don't pre-instrument speculatively. The macros aren't free — `#[derive(Inspect)]` emits per-field accessors; `yield_point!` produces a runtime call site.
2. **Is the alternative materially worse?** A hand-written accessor is sometimes clearer than `#[derive(Inspect)]` when the projection logic is complex. Use the macro when it reduces boilerplate, not just for symmetry.
3. **Does the SUT shape support it?** `Inspect` works for `SkipMap<K, V>` in v1. Other collections, scalars, and atomics aren't supported — for those, hand-write the accessor and revisit when a future slice expands the macro.

If the answers are no / no / no, skip the macro and either hand-write the accessor or defer.

## The three macros — when each fits

### `#[derive(Inspect)]` for concurrent map snapshots

Use when the SUT has one or more `SkipMap<K, V>` fields whose contents the harness wants to read mid-execution. Pick the mode by the V's shape:

**Clone mode** (bare `#[inspect]`) — when V is `Clone` and the harness wants the full data:

```rust
use aristo::instrument::Inspect;
use crossbeam_skiplist::SkipMap;

#[derive(Clone)]
pub struct Transaction { pub seq: u64, pub status: TxStatus }

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect)]
    txs: SkipMap<u64, Transaction>,
}
```

Generates `pub fn inspect_txs(&self) -> Vec<(u64, Transaction)>`.

**Project mode** (`#[inspect(T)]`) — when V isn't `Clone`, or when the harness needs a canonical / subset view:

```rust
pub struct Transaction { pub seq: u64, pub locks: Arc<Mutex<Vec<LockHandle>>> }
pub struct TxnSnapshot { pub seq: u64 }
impl From<&Transaction> for TxnSnapshot { fn from(t: &Transaction) -> Self { TxnSnapshot { seq: t.seq } } }

#[derive(Inspect)]
pub struct MvStore {
    #[cfg_attr(feature = "differential-accessors", inspect(TxnSnapshot))]
    txs: SkipMap<u64, Transaction>,
}
```

Generates `pub fn inspect_txs(&self) -> Vec<(u64, TxnSnapshot)>`. The Arc/Mutex internals stay behind the safety boundary.

Either form accepts `name = "<suffix>"` to override the default method name. Untagged fields are ignored (no codegen).

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

### `yield_point!` for fault-injection points

Use at SUT code locations where a fault-injection harness might want to pause, fail, re-order, or branch:

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

## Convention rules (quick reference)

Full discussion in `docs/instrument-conventions.md`. Summary:

1. **Pick clone or project per V's `Clone`-ability and projection needs.** Default to clone; upgrade to project for non-Clone V or canonicalization.
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

### Using clone mode on a non-Clone V

If V holds an `Arc<Mutex<_>>`, raw `File`, or other non-Clone types, `#[inspect]` (clone mode) fails to compile at the macro-expansion site. Switch to `#[inspect(T)]` (project mode) and define a `From<&V> for T` impl that extracts just the Clone-safe fields.

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
4. **If no**: skip the macro. The harness can hand-write an accessor for one-off cases.

End-of-diff sweeps systematically miss fault-injection points (because they're not visible in the production code shape) and produce inconsistent label / wrapper naming. Instrument inline, with the same rigor you apply to writing intents.

## How to use this skill operationally

When the user says "expose this for testing", "the harness needs to read X", "add a fault-injection point at Y":

1. Identify which macro fits (Inspect / expose_pub / yield_point!).
2. Apply the relevant convention rule (clone vs project; `_for_test` naming; label scheme).
3. Wrap the macro invocation in `#[cfg_attr(feature = "<consumer-alias>", ...)]`.
4. If introducing `yield_point!`, also write or update the harness-side `set_hook` callback.
5. Verify with `cargo check --features <consumer-alias>` that the SUT compiles with instrumentation on and `cargo check` (no features) that it still compiles without.
