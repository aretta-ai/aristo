# ADR: `aristo::instrument` — observation macros for verification

**Status:** Accepted, slice 41 (Phase 2, target v0.2.5).
**Driver:** the differential-testing instrumentation pattern across `aretta-books` + Turso fork. See [`aristo-instrument-handoff.md`](../../../aretta-books/docs-site/design/aristo-instrument-handoff.md) for the consumer-side context.

## Context

Phase 1 of aristo (annotations + index + verify) sits at the **logical** layer of verification — claims about what code does. Differential testing requires a **mechanical** layer too: making private state observable to a harness without leaking it into the public API. Across `aretta-books` and the Turso fork, this work had been hand-rolled per consumer in a `diff-macros` crate; the pattern is general enough to deserve a home in the SDK.

Three macro shapes recurred:

- **Snapshot accessors** over concurrent maps (`SkipMap<K, V>`), needed so the harness can read SUT state mid-execution.
- **Visibility raise** on `pub(crate)` items, so the harness can construct or reference internal types.
- **Yield points** with named labels, so fault-injection harnesses can pause / fail / re-order at controlled call sites.

## Decision

Ship three proc-macros under `aristo::instrument`, gated by an opt-in `aristo_instrument` cargo feature:

- `#[derive(Inspect)]` — generates snapshot accessors over tagged `SkipMap<K, V>` fields.
- `#[expose_pub]` — raises visibility on functions, types, or impl blocks.
- `yield_point!("label")` — emits a runtime call into a thread-local hook for fault-injection harnesses.

## Architecture

```
crates/
├── aristo/                   (meta) — re-exports macros + hosts runtime hook
│   src/instrument/mod.rs    →  pub use aristo_macros::{Inspect, expose_pub, yield_point};
│                                 pub fn set_hook(...) / __yield_point(...)
├── aristo-core/              (lib) — unchanged
├── aristo-macros/            (proc-macro)
│   src/instrument/         →  inspect.rs, expose_pub.rs, yield_point.rs
└── aristo-cli/               (binary) — unchanged
```

### Why the proc-macros extend `aristo-macros` rather than ship in a new crate

Conceptually clean to split logical-layer (`intent` / `assume`) from mechanical-layer (`Inspect` / `expose_pub` / `yield_point!`), but the cost of a fifth workspace member + a fifth crate in every consumer's `Cargo.toml` is real. The macros are feature-gated inside `aristo-macros` (under `aristo_instrument`), so non-instrument consumers pay no extra compile cost. **Locked in slice 36 discussion.**

### Why the runtime hook lives in the meta-crate

The `yield_point!` macro expands to a runtime call (`__yield_point("label")`). The target must live in a regular-lib crate the consumer links against — `aristo-macros` is `proc-macro = true` and can't export non-macro items.

The handoff's original choice of `aristo-core::instrument` proved infeasible: `aristo-core` already depends on `aristo` (because aristo-core uses `#[aristo::intent]` 117 times for dogfooding), so adding `aristo → aristo-core` for the re-export creates a Cargo cycle. Inlining the ~50-line runtime hook in `aristo/src/instrument/mod.rs` alongside the macro re-exports avoids the cycle without adding a workspace member. **Locked in slice 36 discussion.**

### Feature interaction with `aristo_check`

`aristo_check` (gates intent / assume validation codegen) and `aristo_instrument` (gates the instrument surface) are **independent**. They cover orthogonal concerns; consumers turn on whichever they need. Documented in the feature comments on `aristo`, `aristo-macros`, and here.

## Spec deviations from the handoff doc

The consumer-side handoff doc (`aretta-books/docs-site/design/aristo-instrument-handoff.md`) is authoritative for the surface contract, but a few amendments were signed off during slice design and are recorded here. The orchestrator session updates the handoff doc on the `aretta-books` side.

### Inspect derive — clone mode + positional projection type

The handoff specifies only `#[inspect(snapshot = T)]` (project mode). Slice 37 adds:

1. **Clone mode** (bare `#[inspect]`): the macro clones each V into the snapshot Vec directly. No projection type required; field type must implement `Clone`. Simpler for the common case where the V is already a clean data shape.
2. **Positional projection type** (`#[inspect(T)]` instead of `#[inspect(snapshot = T)]`): drops the `snapshot = ` keyword since the first positional arg is unambiguous.

Both forms accept `name = "..."` to override the default `inspect_<field>` method-name suffix.

Rationale: the clone form covers cases where the V is already Clone (and a separate Snapshot type would be redundant); the positional T form is shorter at the call site. Consumers who need the projection (for non-Clone V, or for canonicalization before Lean comparison) still use it. **Locked in slice 37 design discussion.**

### `expose_pub` on impl blocks — included in v1

The handoff's §8 Q4 recommends deferring impl-block support to a later phase ("zero consumer demand so far"). Slice 39 includes it: the macro raises visibility on every `fn` inside the block (associated consts / types untouched). Removes a category of hand-written wrappers that consumers would otherwise need for SUT impl blocks with many `pub(crate)` methods. **Locked in slice 39 design discussion.**

### Trybuild fixture layout

The handoff places `_pending/instrument/<NN>_<name>.md` scenarios for the trybuild matrix. Existing aristo trybuild fixtures live flat in `crates/aristo-macros/tests/ui/{pass,fail}/` with semantic prefixes, no subdirectories, no numeric prefixes. Slice 36 design discussion locked the flat layout for the instrument fixtures: `inspect_clone_basic.rs`, `expose_pub_fn_method.rs`, `yield_point_basic.rs`, etc.

## Consequences

### Positive

- The instrument surface is a single SDK opt-in. Consumers no longer copy-paste a `diff-macros` directory per project.
- Feature gating is symmetric: turn on `aristo_instrument` to get the macros + runtime hook; turn it off for zero compile cost.
- The Turso fork's `aretta-mvcc-differential-accessors` branch can drop ~350 LoC of hand-rolled macro code once aristo v0.2.5 publishes.

### Negative

- Adds one feature flag + one runtime symbol to the meta-crate's public surface. Not free, but bounded.
- v1 supports `SkipMap` only for `Inspect`. Other collection types (`BTreeMap`, `HashMap`, `Vec`) and scalar fields error at the macro level with a clear deferred-to-Phase-3 message. Consumers with other collections hand-write the accessor.

### Out of scope (Phase 3 candidates)

- **Catalog format CLI** (handoff §8 Q5). A future `aristo instrument catalog` subcommand that codifies the `ACCESSORS.md` row schema. Not in v0.2.5; consumer side stays a convention.
- **Inspect beyond SkipMap.** BTreeMap, HashMap, Vec, atomic loads, scalar projections of `Option<T>` — deferred.
- **`yield_point!` in `const fn`.** Detection + clear error. v1 lets the natural rustc error fire ("calls in const fns are limited to ...").
- **Skill suite expansion.** `aristo-instrumenting-philosophy.md` (per CLAUDE.md §10A) lands once feedback cases accumulate. `aristo-instrument-suggestions.md` (parallel to `aristo-intent-suggestions.md`) lands when a second consumer is on board to ground recommendations.

## References

- Consumer-driving doc: [`aristo-instrument-handoff.md`](../../../aretta-books/docs-site/design/aristo-instrument-handoff.md)
- Conventions cheat-sheet: [`docs/instrument-conventions.md`](../instrument-conventions.md) (slice 41)
- Per-pattern cookbook: [`docs/instrument-recipes.md`](../instrument-recipes.md) (slice 41)
- Authoring skill: `crates/aristo-cli/src/skills/aristo-instrumenting.md` (slice 41)
- Slice plan: [`docs/ROADMAP.md`](../ROADMAP.md) Phase 2.
