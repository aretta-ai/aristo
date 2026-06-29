# `aristo::instrument` — conventions

Five rules for using the instrument surface well. Each rule is one sentence, then a `Why:` paragraph and a `How to apply:` line. Skim or grep.

> Companion docs: [`instrument-recipes.md`](./instrument-recipes.md) (per-pattern cookbook), [`decisions/instrument-surface.md`](./decisions/instrument-surface.md) (architecture ADR).

## Rule 1. Pick clone or projection per the field

`#[derive(Inspect)]` is type-agnostic — it never inspects the field's type. Each `#[inspect]`-tagged field becomes a `pub fn inspect_<field>(&self)` returning an owned snapshot. Two modes, chosen at the attribute:

- `#[inspect]` — clone mode. Clones the whole field and returns its declared type verbatim. Use when the field is `Clone` and the harness wants the full data. `#[inspect(name = "x")]` overrides the method suffix.
- `#[inspect(ret = T, with = <projector>)]` — projection mode. Hands the whole field to a `Fn(&FieldType) -> T` projector — a named function (reuse / complex bodies) or an inline closure (one-liners; no parameter annotation needed). Use for non-`Clone` fields (atomics, lock guards, raw handles), foreign types, or any canonicalized / filtered / fanned-out view. `ret` is required and echoed verbatim — a proc-macro cannot infer a closure's return type.

**Why:** clone is zero-boilerplate but only fits `Clone` fields and applies no transformation. Projection covers everything else and, because the projector sees the whole field, can filter and fan-out — strictly more than a per-entry mapping. It is also orphan-safe: the emitted method is inherent on your struct, so a foreign field type is never the `Self` of a foreign trait.

**How to apply:** default to `#[inspect]` for a `Clone` field; switch to `#[inspect(ret = T, with = <projector>)]` when the field isn't `Clone` or the harness needs a transformed / canonical view. The positional `#[inspect(T)]` and `snapshot = T` forms were removed in v0.3.0 — migrate them to `ret = …, with = …`. This was a **generalization, not a loss**: the old form was `SkipMap`-only, whereas `ret = T, with = |f| …` projects *any* one field — scalars, `Option`, atomics (`|a| a.load(Acquire)`), lock-guarded reads. A comment claiming 0.3.0 "dropped" a scalar sub-shape predates this (or refers to a different, in-tree macro); migrate it to projection mode rather than hand-writing the accessor.

**Caveat — clone returns the field's type verbatim.** If that type, or a type reachable inside it (a private enum in a field), is crate-private, the harness receives the value but cannot name it for an annotation nor `match` its private variants from outside the crate (`error[E0603]`). Project such fields instead: the `with` closure runs inside the SUT where the private type is in scope, so only a harness-nameable `ret` (primitives, or a `pub` snapshot struct) crosses the boundary.

## Rule 2. Name `expose_pub` function wrappers with a `_for_test` suffix

`#[expose_pub(as = "<name>")]` on a `pub(crate)` function requires a wrapper name. Convention: the wrapper name carries a `_for_test` (or `_for_harness`) suffix.

**Why:** the wrapper is reachable from outside the crate when the feature is on; calling it from production code would silently couple production to the harness surface. The suffix makes the boundary visible in every call site — `grep _for_test` finds every leak.

**How to apply:** `#[expose_pub(as = "new_for_test")]` for `pub(crate) fn new`. Avoid `pub_new` / `external_new` — those look like first-class API.

## Rule 3. Don't rename types — `expose_pub` raises visibility in place

`#[expose_pub]` on `enum` / `struct` / `type` / `impl` FORBIDS `as = "..."`. The macro raises the existing item's visibility to `pub` and tags it `#[doc(hidden)]`. The name stays the same.

**Why:** type names are referenced from every call site, every test, every consumer. Renaming would cascade through the whole crate. The wrapper convention from Rule 2 only makes sense when the wrapper is a *distinct function* — for types, there's nothing distinct to make.

**How to apply:** `#[expose_pub] pub(crate) enum ParsedOp { ... }`. The type stays `ParsedOp` everywhere; only its visibility lifts when the feature is on.

## Rule 4. Label `yield_point!` calls with `<fn>.before_<action>` / `<fn>.after_<action>`

`yield_point!("label")` accepts any `&'static str`, but consistent labelling matters because the harness selects which point to inject faults at by label.

**Why:** labels are the only identifier the harness sees. Inconsistent or vague labels (`yield_point!("done")`, `yield_point!("checkpoint")`) make harness code ambiguous and fragile to source rearrangement.

**How to apply:** the label scheme is `<fn-name>.<before|after>_<action-being-instrumented>`. Examples: `"write_header.before_fsync"`, `"commit.after_log_sync"`, `"flush.before_unlock"`. One label per call site, no duplicates within a function.

**Caveat — `yield_point!` is for I/O / fault boundaries, not passive probes.** A pure in-memory data structure has nothing to pause or fail, so a yield point there can only announce that an event happened — and its `&'static str` label can't carry *which* item or *how big*. To observe such state (a representation switch, a size threshold), use an `Inspect` projection instead: it returns typed, owned data the harness asserts on directly (Recipe 10), with no global `static` to route observations through. Reach for `yield_point!` only where there is a real I/O or fault boundary to inject at.

## Rule 5. Gate macro invocations with `cfg_attr` for production-build cost zero

The `aristo_instrument` feature isn't on by default. When off, the macros aren't exported, so call sites must be feature-gated by the consumer.

**Why:** leaving an `#[expose_pub]` or `#[derive(Inspect)]` ungated forces every downstream build to enable `aristo_instrument` just to compile. Gating each invocation keeps the feature truly opt-in.

**How to apply:**
```rust
#[cfg_attr(feature = "aristo-instr", aristo::instrument::expose_pub(as = "new_for_test"))]
pub(crate) fn new(buf_size: usize) -> Self { ... }
```
`yield_point!` / `fault_point!` calls follow the same pattern via `cfg!` or a `#[cfg(feature = "...")]` block around the call site. Consumers alias a feature name onto `aristo_instrument` in their own `Cargo.toml` — `aristo-instr = ["aristo/aristo_instrument"]`. The SDK accepts any alias, but the **aretta consumers standardize on `aristo-instr`** (used throughout these recipes); match it unless you have a reason not to.
