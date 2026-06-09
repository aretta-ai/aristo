# `aristo::instrument` — conventions

Five rules for using the instrument surface well. Each rule is one sentence, then a `Why:` paragraph and a `How to apply:` line. Skim or grep.

> Companion docs: [`instrument-recipes.md`](./instrument-recipes.md) (per-pattern cookbook), [`decisions/instrument-surface.md`](./decisions/instrument-surface.md) (architecture ADR).

## Rule 1. Pick clone or project per V's `Clone`-ability and projection needs

`#[derive(Inspect)]` supports two modes per tagged `SkipMap<K, V>` field:

- `#[inspect]` — clone mode. Each V is cloned as-is. Use when V is `Clone` and the harness wants the full data shape.
- `#[inspect(SnapshotType)]` — project mode. Each V is passed through `From<&V>::from`. Use when V isn't `Clone` (raw handles, `Arc<Mutex<_>>`, etc.) OR the harness needs a canonicalized / subset view for cross-implementation comparison.

**Why:** clone is cheaper for the user (zero boilerplate) but inflexible. Project covers the harder cases but requires a per-snapshot type + `From` impl. Picking the wrong mode produces either redundant types (project where clone suffices) or compile errors (clone where V isn't `Clone`).

**How to apply:** default to `#[inspect]`. Upgrade to `#[inspect(T)]` when `Clone` fails or the harness's equivalence check needs canonicalization at the projection point.

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

## Rule 5. Gate macro invocations with `cfg_attr` for production-build cost zero

The `aristo_instrument` feature isn't on by default. When off, the macros aren't exported, so call sites must be feature-gated by the consumer.

**Why:** leaving an `#[expose_pub]` or `#[derive(Inspect)]` ungated forces every downstream build to enable `aristo_instrument` just to compile. Gating each invocation keeps the feature truly opt-in.

**How to apply:**
```rust
#[cfg_attr(feature = "differential-accessors", aristo::instrument::expose_pub(as = "new_for_test"))]
pub(crate) fn new(buf_size: usize) -> Self { ... }
```
`yield_point!` calls follow the same pattern via `cfg!` or a `#[cfg(feature = "...")]` block around the call site. Consumers alias the feature name they prefer onto `aristo_instrument` in their own `Cargo.toml` (see handoff §2.4).
