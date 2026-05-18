# `verify="test"` (and `verify="full"`) — deferred design notes

**STATUS: deferred to post-MVP.** Originally slices 24–26 of milestone E; paused
2026-05-17 because the spec schema and the injection mechanism are tightly
coupled and the injection mechanism needs more design before we commit to a
spec format.

This file captures the research already done, the architectural options
surveyed, the recommendation in flight, and the specific open questions that
must be answered before implementation can start. The purpose is to preserve
intellectual context so future-us doesn't re-investigate the same prior art.

When this work is resumed, promote a slice into `ROADMAP.md` and reference
this file as the design backing.

## What's deferred

- Slice 24: `verify="test"` free path (spec mining + persistence)
- Slice 25: `aristo_verify` cargo feature in `aristo-macros` (injection at compile time)
- Slice 26: J4 free-tier `verify="full"` downgrade (depends on 24)
- The `[full]` body in the spec schema (formal-prover specs)

The neural verification path (slice 23) is unaffected and remains in place.

## Vocabulary lock

This work introduced (and we agreed to enforce) the strict distinction between:

- **Annotation** — human-authored, natural-language description in source via
  `#[aristo::intent(...)]` etc. Read by humans, agents, reviewers.
- **Specification** — machine-checkable formalization of (part of) an
  annotation. Lives in `.aristo/specs/<spec-id>.spec`, never in source.
  Generated on demand; not for humans to read or write.

This terminology MUST be preserved in all subsequent design discussion. When
the work resumes, document the distinction in `aristo/docs/concepts/` as the
first deliverable before any implementation.

## Spec schema — proposed but not committed

Sketched during the design discussion. Captured here so we don't redo the
schema work from scratch; tagged with the open questions that determine
whether it stands.

```toml
# .aristo/specs/<spec-id>.spec

[meta]
schema_version = 1
spec_id = "spec_xVuG9F3a"           # opaque; same shape as `aret_*` annotation ids
target_method = "test"              # closed enum: test | full
generated_by = "aristo-mine-assertions@v0.0.6"
generated_at = "2026-05-17T..."
human_reviewed = false              # true → sacrosanct (never auto-overwrite)

# Annotations this spec formalizes — ≥1 required (invariant S1).
[[links]]
annotation_id = "balance_no_duplicate_cells"
covered_text_hash = "sha256:..."    # validator: current index text_hash must match
relation = "instantiates"           # closed enum: instantiates | strengthens | one-of-many

# Program-location anchor.
[anchor]
file = "crates/aristo-core/src/btree.rs"
function = "balance_non_root"
body_hash = "sha256:..."            # function body at gen time; drift → stale
injection_point = "post"            # closed enum: pre | post | inline
return_binding = "balance_output"   # bind return value for post-specs

# Method body — exactly one of [test] / [full] matching meta.target_method.
[test]
predicate = "balance_output.cells().count() == balance_output.cells().unique_by(|c| c.id).count()"
required_imports = ["itertools::Itertools"]
panic_message = "balance must not duplicate cells"
```

Schema-level invariants we agreed on:

- **S1 — no bare specs.** `links = []` is a validator-rejected state.
- **S2 — not all annotations need specs.** Most don't.
- **S3 — many-to-many.** One spec can cover N annotations; one annotation can
  decompose into M specs at different anchors.
- **S4 — programmer-invisible.** Specs live in `.aristo/specs/`, never inline.
- **S5 — AST-anchored, hash-anchored.** Drift on link or anchor renders stale.

## Architectural options for spec injection — five surveyed

| # | Approach | Where spec lives | Injection mechanism | Inline support | Staleness detection |
|---|---|---|---|---|---|
| I | Proc macro reads .spec at expand time | `.aristo/specs/*.spec` | `#[aristo::intent]` macro under `aristo_verify` feature wraps body | No (pre/post only) | Two-layer: stamp-time (covered_text_hash, anchor.body_hash) + macro-expand-time (body_hash recheck → compile_error) |
| II | build.rs generates external test harness | `.aristo/specs/*.spec` | Build script generates `tests/aristo_verify_harness.rs` calling target fns | No (observable only) | build.rs validates; mismatch → build error |
| III | Source rewriter (separate cargo subcommand) | `.aristo/specs/*.spec` | `aristo verify --test` rewrites source pre-`cargo test` | Yes (full flexibility) | Rewriter validates; rewrites to temp dir |
| IV | Hybrid: macro for pre/post, build.rs for arbitrary anchors | `.aristo/specs/*.spec` | Specs choose mechanism via injection_point | Yes (via harness) | Both layers |
| V | Programmer-placed checkpoint macro: `aristo::check!(spec_id)` | `.aristo/specs/*.spec` | Macros at user-placed sites pull spec bodies | Yes | Spec ↔ checkpoint id binding |

Killer concerns per option:

- **I:** inline-mid-function injection is hard (proc macros can wrap a body
  but not cleanly inject at arbitrary internal points). Accept the
  constraint and ship pre/post-only.
- **II:** loses internal-state access — can only assert on observable
  input/output. Many invariants ("internal counter never negative") can't
  be expressed.
- **III:** source-rewrite paths are fragile — debugger line numbers, comments,
  idempotency are all hard. Cargo-fix has years of experience with how
  this hurts.
- **IV:** doubled surface area.
- **V:** breaks invariant S4 (programmer-invisible). The checkpoint macros
  ARE programmer-facing.

## Prior art — grouped by spec/source separation

Group A — specs IN source (zero staleness, but breaks S4):
`contracts` crate (Rust), Prusti, Creusot, JML (Java), D's contracts, Eiffel,
Ada/SPARK. Universal pattern: contract attribute on the function, macro
wraps the body. Cited as the gold standard for "specs that can't go stale."

Group B — specs OUTSIDE source, manually maintained (severe staleness):
TLA+, Alloy, Promela, Frama-C/ACSL (in practice). Common failure mode: model
becomes shelf-ware.

Group C — specs OUTSIDE source, machine-anchored (where Aristo wants to be):
Coq/Lean proof scripts alongside .v/.lean (CompCert, seL4) — staleness via
proof obligation regeneration. Property-based fixture corpora (hypothesis-
database, cargo-fuzz). Build.rs codegen patterns (prost, tonic, wasm-bindgen).

The closest direct analog is `include_str!` + macro: external file read at
macro expansion time. Combined with the `contracts`-crate body-wrap pattern,
this is the basis for Option I.

## Recommendation in flight (NOT yet committed)

**Option I, scoped to pre/post injection only.** Sliced in three steps when
work resumes:

1. Spec schema + types in `aristo-core::spec` + mechanical validator
   (mirrors `aristo-core::proof` shape).
2. Skill + dispatch: `aristo-mine-assertions` bundled skill;
   `.aristo/pending-test.toml` request file; `--apply-specs` validator pass
   on returned specs.
3. `aristo_verify` cargo feature in `aristo-macros` — the macro reads matching
   specs at expand time, computes current `body_hash`, compares to spec's
   stored `anchor.body_hash`, emits the wrapped body with predicates.

The `[full]` method, `injection_point = inline`, and "anchor can be a
non-annotated callee" extensions stay closed-enum-but-NotImplemented in
the schema, expandable when Phase 2 (paid tier + formal verification) lands.

## Open questions — must be answered before implementation

These four questions are the blockers. Without explicit answers, implementation
will either hard-code an opinion that doesn't survive review or stall on
back-and-forth in the middle of slice 24.

### 1. Macro behavior on missing spec file (under aristo_verify feature)

When `cargo test --features aristo_verify` runs and the macro on a
`verify="test"` annotation finds no matching spec on disk:

- (a) silent skip (only check existing specs)
- (b) warn-and-skip (cargo warning per missing spec; test continues)
- (c) compile_error per missing spec (forces `aristo verify` before tests)

Tradeoff: (c) gives strongest gate but blocks CI on freshly-cloned workspaces
where mining hasn't run yet. (a) hides genuine "spec was never generated"
problems. (b) splits the difference.

In-flight lean: (b).

### 2. `injection_point = inline` — drop from schema or keep as NYI?

- (a) keep in the closed enum; validator rejects with "slice 25+ NYI"
- (b) drop entirely until shipped

In-flight lean: (a) — same pattern as `verify="full"` (enum-present, returns
NotImplemented today). Open enum to signal design intent.

### 3. What does the mining skill return when it CAN'T formalize an annotation?

The skill can't always produce a spec — the annotation may not be expressible
as a pre/post predicate (mid-function invariant, abstract design choice).
Three options for the failure surface:

- (a) Skill returns an "inconclusive spec" — like an inconclusive proof,
  persisted to `.aristo/specs/`, but flagged as un-injectable
- (b) Skill returns NOTHING for that entry; SDK has no record of "tried and
  failed" — next verify retries forever
- (c) Skill returns a separate "attempt record" persisted to a new dir
  like `.aristo/spec-attempts/<annotation-id>.attempt`, listing what was
  tried and why it failed. Next verify only retries on annotation text drift.

In-flight lean: (c) — mirrors the structure of inconclusive proofs without
polluting the specs/ directory with non-injectable artifacts.

### 4. Anchor flexibility — must the anchor function carry the linked annotation?

- (a) Strict: `anchor.function` must be a function bearing one of the linked
  annotations. Mining skill respects this constraint.
- (b) Loose: anchor can be any function in the same module / call graph.

(a) is forced by Option I (the macro is on the annotated function — only that
function can be wrapped). (b) would require the build.rs harness path.

In-flight lean: (a) for slice 24; revisit if a real case demands loose mode.

## Pre-implementation work needed (the blockers)

When this work resumes, before any code lands:

1. **Answer the four open questions above** — explicit decisions, written
   down. Without these, the implementation will stall.

2. **Write the concepts doc** at `aristo/docs/concepts/annotations-and-specifications.md`
   defining the annotation/specification vocabulary and the five S1–S5
   invariants. Reference it from CLAUDE.md so reviewers stay on the
   terminology.

3. **Lifecycle diagram** added to the Mermaid HTML viewer as section 5,
   showing how spec states transition (Pending → OnDisk → Fresh → Injected
   → Tested|Refuted; Fresh → Stale; Stale → Pending) and how they interact
   with the annotation lifecycle (cascade on annotation removal; stamp-time
   spec staleness; macro-time hash recheck).

4. **Make a deliberate decision on whether to do the `.candidate` flow for
   `human_reviewed` specs** in this initial slice or defer it. Either way,
   document the decision.

5. **Decide whether build.rs harness (Option II) belongs alongside the
   macro path** for cases where the annotation site can't host the
   injection. (Probably no — option I + the strict anchor constraint
   handle slice 24's scope. Document why if we say no.)

## Why we paused

The single observation that forced the pause: the spec schema and the
injection mechanism are not independent. Choosing the schema (e.g.,
`injection_point=inline`) commits us to an injection mechanism that supports
it (e.g., Option III source rewriter). Choosing the injection mechanism
(e.g., Option I macro-time) constrains the schema (e.g., pre/post only).

You can't design one cleanly without the other. The right move is to settle
the open questions FIRST, then write the schema and the injection together
as one design artifact.

## Pointers for resumption

- This file: design-discussion record, open questions, recommendation in flight
- `aretta-sdk/docs/verification-research-notes.md`: deferred autoformalization
  techniques for the NEURAL verification path (separate concern; preserved
  for the same reason)
- Slice 23 (verify="neural") at commits `730bbb8` through `3d3de45`: the
  precedent for how to structure a verify-method slice — pending file,
  skill bundle, validator, apply step, mechanical-only validation
- Mermaid viewer section 4 (proof state lifecycle): the shape the spec
  lifecycle diagram should mirror when added
