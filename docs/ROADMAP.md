# Aristo SDK — roadmap

`docs/ROADMAP.md` is the plan of record. Per CLAUDE.md §9, all work is governed by this file (or, for a focused phase, a sibling `docs/PHASE-N-PLAN.md`).

**Phase 1** (the offline Rust SDK MVP — annotations, index, verify=neural, doc/graph/badge surfaces) shipped across milestones A–H and the v0.2.x patch series, closing at **v0.2.4**. See git history and CHANGELOG.md for the full slice ledger (slices 1–35 + post-MVP refinements).

**Phase 2** opens here.

## How this roadmap relates to CLAUDE.md

CLAUDE.md is the source of truth for **how we work** (commit size, semantic messages, test-first, full-check gating, plan-driven, annotation discipline, release cadence). This roadmap is the source of truth for **what we build, in what order**. When they disagree, CLAUDE.md wins.

Two CLAUDE.md rules are load-bearing for this phase:

- **§12 / §12A — Specifications are the truth; promote spec before impl.** Each slice's trybuild cases (the `tests/ui/{pass,fail}/*.rs` fixtures) are the spec for that slice; they land before the codegen that turns them green.
- **§11 — Release cadence.** Phase 2 closes with `chore(release): v0.2.5` + `git tag v0.2.5`. Per-slice commits stay small and semantic.

---

## Phase 2 — instrument surface → v0.2.5

Goal: ship `aristo::instrument::*` — three macros (`Inspect` derive, `expose_pub` attribute, `yield_point!` function-like) — making private state observable to verification harnesses without copy-pasted macros per consumer.

Aristo's umbrella story extends: **annotations** (intent/assume) document logical invariants; **instrumentation** (instrument) makes mechanical state observable. Both serve verification; both ship from the same SDK.

Consumer-driving doc: `../aretta-books/docs-site/design/aristo-instrument-handoff.md`. Design archive lands in slice 41 at `docs/decisions/instrument-surface.md`.

### Architecture summary

- **No new workspace crate.** The three proc-macros live alongside `intent`/`assume` in `aristo-macros`, gated by an opt-in `aristo_instrument` cargo feature.
- **Runtime hook lives in `aristo` (the meta-crate)** — a thread-local function pointer (`set_hook` / `__yield_point`) that the `yield_point!` expansion calls. Putting it directly in the meta-crate avoids a Cargo cycle (`aristo-core → aristo`).
- **Feature gating.** `aristo_instrument` is independent from `aristo_check` (orthogonal concerns; documented in the ADR). Consumers alias their preferred flag name onto `aristo_instrument` in their own Cargo.toml (handoff §2.4).

### Slice plan

| Slice | Deliverable | Trybuild cases promoted | Commits |
|---|---|---|---|
| **36** | Crate scaffolds + feature wiring. Recreate `docs/ROADMAP.md`. Add `aristo_instrument` feature to `aristo-macros` + `aristo` (passthrough). Empty proc-macro stubs for `Inspect` / `expose_pub` / `yield_point!`. Runtime hook stub in `aristo/src/instrument/` (no-op default, working `set_hook` + `__yield_point`). Smoke test verifies re-exports resolve. Add `crossbeam-skiplist` as dev-dep of `aristo-macros`. | — | 2 |
| **37** | `Inspect` derive — `snapshot = T` sub-shape. Field-attr parsing, `SkipMap` detection by trailing path segment, owned-snapshot codegen. NO sort. NO Phase 1 expose shape. | `pass/inspect_snapshot_{basic, with_name, two_fields}.rs`; `fail/inspect_snapshot_{no_skipmap, unnamed_fields}.{rs,stderr}` | 2–3 |
| **38** | `expose_pub` attribute — function form. Item-kind detection via `syn::Item`; emit `pub` wrapper for `ItemFn` / `ImplItemFn`; require `as = "..."`. Receiver inference (`&self`, `&mut self`, none), generics + lifetimes passthrough. | `pass/expose_pub_fn_{basic, method}.rs`; `fail/expose_pub_fn_missing_as.{rs,stderr}` | 2–3 |
| **39** | `expose_pub` attribute — type + impl-block forms. `ItemEnum` / `ItemStruct` / `ItemType`: emit cfg-gated `pub` twin declaration; forbid `as = "..."`. ImplBlock: raise visibility on every method inside. | `pass/expose_pub_type_{enum, struct, impl_block}.rs`; `fail/expose_pub_type_extra_as.{rs,stderr}` | 2–3 |
| **40** | `yield_point!` function-like macro + runtime hook completion. Feature-gated expansion to `aristo::instrument::__yield_point("label")`. `const fn` detection → clear error. Trybuild covers feature-off no-op path; aristo unit tests cover feature-on hook dispatch. | `pass/yield_point_basic.rs` + aristo `tests/hook_dispatch.rs` | 2 |
| **41** | ADR (`docs/decisions/instrument-surface.md`) + conventions doc (`docs/instrument-conventions.md`) + per-pattern recipes (`docs/instrument-recipes.md`) + authoring skill (`crates/aristo-cli/src/skills/aristo-instrumenting.md`, registered in `skills/mod.rs`). | — | 3–4 |
| **42** | Release v0.2.5. Workspace version bump; promote `[Unreleased]` → `[v0.2.5]` with date; commit body summarises slices 36–41; tag and push `v0.2.5`. | — | 1 |

Total: **13–18 commits, ~3–4 weeks of focused work** under aristo's CLAUDE.md discipline.

### Out of scope for v0.2.5

- **Catalog format CLI** (handoff §8 Q5). A future `aristo instrument catalog` subcommand that codifies the `ACCESSORS.md` row schema is a Phase 3 candidate; consumer side stays a convention for now.
- **Skill suite extras.** `aristo-instrumenting-philosophy.md` (per CLAUDE.md §10A) lands once feedback cases accumulate. `aristo-instrument-suggestions.md` (parallel to `aristo-intent-suggestions.md`) lands when a second consumer is on board to ground recommendations.

### Implementation debt landed in v0.2.5 / v0.2.6 (not deferred design)

- **`Inspect` field-shape widening — RESOLVED in v0.3.0 (2026-06-28).** The slice-37 locked API was type-agnostic. The v0.2.5 / v0.2.6 implementation narrowed to `SkipMap<K, V>` fields only as a shipping shortcut (the macro errored with `"only supports SkipMap<K, V> fields in v1"` for anything else). v0.3.0 closes the debt — but by making the derive genuinely type-agnostic (whole-field clone / `with`-projection) rather than the per-shape codegen originally planned, and **breaking** the old `SkipMap`-only `#[inspect(T)]` / `snapshot = T` forms in the process (replaced by `#[inspect(ret = T, with = <projector>)]`). See `docs/decisions/instrument-surface.md` § "Implementation debt".

### Verification target

End-to-end success criterion: the Turso fork at `../turso-mvcc-diff` (branch `aretta-mvcc-differential-accessors`) builds clean under `cargo check --features differential-accessors -p turso_core --lib` once `aristo` v0.2.5 is published and the fork's `Cargo.toml` dep resolves. That run is owned by a separate orchestrator session (handoff §6); this branch's job is to ship the surface.

---

## Future phases (sketch)

- **Phase 3 — catalog tool + skill suite expansion.** `aristo instrument catalog` CLI; `aristo-instrumenting-philosophy.md` from accumulated cases; `aristo-instrument-suggestions.md`. (The "Inspect beyond SkipMap" item that previously appeared here is **implementation debt**, not Phase 3 design — see the "Implementation debt landed in v0.2.5 / v0.2.6" section above.)
- **Phase 4 — second AI-consumer onboarding.** HelixDB or future SUT integration; mining new patterns from a second consumer's feedback loop.

These are sketches, not commitments — Phase 2's outcome will reshape them.
