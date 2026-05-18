# Session B — start context

**Temporary handoff file.** Delete on merge of the session-B branch back to
`main`. Authored 2026-05-18 by Session A.

You are **Session B**, running in a parallel git worktree of the `aristo`
repo. Session A continues on `main` working on the review-sessions
substrate arc (slices 27.5 / 27.6 / 27.7). You own two slices that
parallelize cleanly with that work: **slice 28 (`aristo doc`)** and
**slice 31 (`aristo badge`)**. The roadmap rows for both are marked
`[Assigned to session B]`.

This file gives you everything you need to start cold without re-asking
the human. Read it end-to-end before touching code.

---

## 1. Read these first (in this order)

1. **`CLAUDE.md`** at the repo root — this is **law**. Especially §1 (commit
   size), §2 (semantic messages), §3 (CHANGELOG every commit), §4 (test-
   first), §6 (full check suite before every commit), §8 (small batches),
   §10 (annotation discipline — annotate as you write), §12 (specs are
   the truth — never edit a spec to match impl), §12A (promote the spec
   FIRST at slice start, not the end).
2. **`docs/ROADMAP.md`** — your two slice rows (28 and 31) plus the
   surrounding milestone context.
3. **`docs/TESTING.md`** — toolchain (trycmd / assert_cmd / predicates),
   `_pending/` → `active/` promotion protocol, sandbox convention
   (`.in/` + `.out/` siblings for stateful commands).
4. **Design source-of-truth** — these are in the sibling `aretta-sdk`
   repo, accessed via the repo-root symlinks documented in CLAUDE.md §9:
   - `../aretta-sdk/docs/DECISIONS.md`
   - `../aretta-sdk/docs/TOOLS.md`
   - `../aretta-sdk/docs/mockups/10-doc-and-graph/` (slice 28 mockups —
     `samples.md` is especially important; it shows the exact output
     shape per-annotation markdown is expected to produce)
   - `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`
     (slice 31 badge mockup)
   - `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md`
     (general CLI shape, --json / --check / --strict patterns)

If any of these contradict each other, surface to the human — **do not**
silently pick one (CLAUDE.md §12 + §9).

---

## 2. Your two slices

### Slice 28 — `aristo doc` (milestone G, targets v0.0.8)

- **Goal:** generate per-annotation markdown to `.aristo/doc/<id-safe>.md`
  from the live `.aristo/index.toml`. `<id-safe>` replaces `:` with `__`
  (same convention as `.proof` / `.critique` files — be consistent).
- **Flags:** `--include-status` (bake B5b status into the rendered MD —
  but B5b classification is Phase-2; for v0 use whatever status the
  index already carries), `--summary` (write project-level
  `_summary.md`), `--check` (CI gate; non-zero on drift).
- **Incremental writes:** if `<id>.md` exists and the source annotation
  hasn't drifted, skip the write. Use `text_hash` / `body_hash` from the
  index as the staleness anchor (same pattern as `aristo critique` and
  the verify pipeline already use — read `commands/critique/apply.rs`
  and `commands/verify/` for examples).
- **Pending scenarios to promote** (per CLAUDE.md §12A — promote BEFORE
  writing impl):
  - `crates/aristo-cli/tests/cmd/_pending/doc_first_run.md`
  - `crates/aristo-cli/tests/cmd/_pending/doc_incremental.md`
  - `crates/aristo-cli/tests/cmd/_pending/doc_summary.md`
  - `crates/aristo-cli/tests/cmd/_pending/doc_check_fails.md`
  - `crates/aristo-cli/tests/cmd/_pending/doc_include_status.md`
  - `doc_include_graph.md` is one of the 6 doc scenarios but ROADMAP
    says slice 29 (`aristo graph`) lands the `--include-graph` half via
    slice composition — **leave that scenario in `_pending/`** unless
    you also pick up slice 29.
- **Promotion is byte-for-byte** per CLAUDE.md §12. If a scenario won't
  pass without rewording, **fix the impl, not the spec.**
- **Currently a stub:** `Commands::Doc` in `crates/aristo-cli/src/lib.rs`
  routes to `not_yet("aristo doc", "slice 28")`. That stub goes away in
  your first impl commit. See "Shared files" below for the test that
  also needs re-pointing.

### Slice 31 — `aristo badge` (milestone H, targets v0.1.0)

- **Goal:** read `.aristo/index.toml`, compute metrics (`aristos-count`,
  `verification-rate`), emit an SVG badge.
- **Flags:** `--out <path>` (write to file) or stdout (default), `--style`
  in `{flat, flat-square, for-the-badge}` (the three shields.io-compatible
  styles).
- **Offline-only.** `--strict` is server-side and stays deferred to
  Phase 2. Do not stub it; do not add the flag.
- **Pending scenario to promote:**
  - `crates/aristo-cli/tests/cmd/_pending/stale_preflight_on_badge.md`
  - The badge portion of
    `crates/aristo-cli/tests/cmd/_pending/lifecycle_ship_with_doc_and_graph.md`
    (this composite scenario also covers doc + graph — it can only fully
    promote once slice 29 also ships; for v0.0.8/v0.1.0 work, treat as
    deferred to a follow-up composite verification slice).
- **Currently a stub:** `Commands::Badge` routes to `not_yet("aristo
  badge", "slice 31")` — same shape as `Doc`.

### Recommended order

Do **slice 28 first**, then slice 31. Slice 28's incremental-write logic
and index-reading scaffold can be reused (carefully, no premature
abstraction — see CLAUDE.md §7 + the "no speculative scaffolding" rule)
for slice 31's index read. Both close on their own commits; do not
bundle them.

---

## 3. What Session A is touching — DO NOT MODIFY THESE PATHS

Session A is implementing the review-sessions substrate + critique v1
extensions. Conflicts will surface fast if you stray into:

- `crates/aristo-cli/src/commands/critique/**` — all of it
- `crates/aristo-cli/src/commands/verify/**` — all of it
- `crates/aristo-cli/src/skills/aristo-critique.md` — skill body
- `crates/aristo-cli/src/skills/aristo-neural-verify.md` — skill body
- `crates/aristo-cli/src/commands/install_skills.rs` — hook installation
  changes for the substrate
- Anything under `crates/aristo-cli/src/session/` or
  `crates/aristo-core/src/session/` (these don't exist yet — Session A
  is creating them)
- `crates/aristo-core/src/critique/**` — critique schema changes for v1
  caching fields and per-finding disposition
- `crates/aristo-core/src/index/**` — Session A is adding caching
  fields here (`last_critiqued_at_text_hash`, etc.)
- `.aristo/sessions/**` — runtime artifact, gitignored by Session A
- `docs/decisions/review-sessions.md` and
  `docs/decisions/critique-finding-disposition.md` — Session A may
  amend during implementation

You CAN read all of these freely — just don't write to them.

---

## 4. Files BOTH sessions will touch — coordination rules

These are unavoidable shared edits. They have textual conflicts but
the conflicts are trivial. Resolution policy:

| File | What Session A adds | What you add (Session B) | Merge strategy |
|---|---|---|---|
| `crates/aristo-cli/src/lib.rs` | new `Commands::Session(SessionArgs)` variant + dispatch arm | un-stub `Commands::Doc` and `Commands::Badge` (currently route to `not_yet`); add flag structs | 3-way merge by hand; both edits append to the `Commands` enum and its match — pick both, alphabetize if needed |
| `crates/aristo-cli/src/commands/mod.rs` | `pub mod session;` | `pub mod doc;` and `pub mod badge;` | 3-way merge by hand; both edits are additive |
| `crates/aristo-cli/tests/binary_smoke.rs` | (none expected) | `defined_but_unimplemented_subcommand_exits_64` currently points at `aristo doc` — when slice 28 ships, re-point to `aristo graph` (slice 29, still stubbed); when slice 31 ships, badge no longer reachable so no further change | you (Session B) own this edit — it's purely yours; Session A doesn't touch it |
| `CHANGELOG.md` `[Unreleased]` | one bullet per commit | one bullet per commit | both append under the same heading; merge picks both |
| `Cargo.toml` (workspace `version`) | release bump only at milestone close | (none — milestone-close is Session A's call) | leave to Session A |

**If a conflict surfaces that isn't covered here** — stop, write it to
this file under a new "Coordination needed" section, and ping the
human before resolving. CLAUDE.md §7 ("surface widening blast radius
before silently fixing") applies.

---

## 5. Worktree + branching

Set up your worktree from `main` (currently at `c8b7d8b`, the slice-27
landing commit). The human will create the worktree for you; you don't
need to run `git worktree add` yourself.

- **Branch:** `session-b/slice-28-doc` for slice 28; cut a separate
  branch `session-b/slice-31-badge` for slice 31 after 28 merges.
  Do NOT bundle both slices on one branch — CLAUDE.md §1 + §8.
- **Push cadence:** push after every commit per CLAUDE.md §8 "small
  batches — ship fast, ship incomplete, ship green." Each commit must
  pass the full §6 check suite locally before push.
- **PR target:** `main`. Open the PR when the slice is complete (every
  promoted scenario green + §6 clean). The human merges; you do not
  self-merge.
- **Rebase posture:** if `main` advances while you're working (Session A
  lands commits), rebase your branch onto `main` regularly — don't let
  conflict surface area grow.

---

## 6. Patterns to mirror (do not re-invent)

The aristo CLI has a stable shape. New commands follow it. Closest
analogs for your two slices:

- **Module layout for slice 28** — `crates/aristo-cli/src/commands/doc/`
  with `mod.rs` (entry + arg struct + dispatch), and submodules as
  needed. Mirror `crates/aristo-cli/src/commands/critique/` (which has
  `mod.rs`, `pending.rs`, `submit.rs`, `apply.rs`, `validator.rs`) for
  the multi-file shape. If slice 28 fits in one file, that's fine —
  see `commands/show.rs` and `commands/lang.rs` for single-file
  examples.
- **Module layout for slice 31** — single file
  `crates/aristo-cli/src/commands/badge.rs` is fine (no subagent
  orchestration, no queue). Mirror `commands/lang.rs` or `commands/init.rs`
  for the single-file shape.
- **Reading the index** — there's already an index-loader in
  `aristo-core`; find it via `rg "index.toml" crates/aristo-core/src`
  and reuse rather than re-implementing. Do not roll your own TOML
  parse path.
- **`--check` mode** — `aristo lint --check` (slice 20) and `aristo
  stamp --check` (slice 17) are the precedents. Read both before
  designing slice 28's `--check`. Same exit-code conventions apply.
- **Freshness preflight** — `aristo status` (slice 19) wraps the
  shared internal preflight. Both `doc` and `badge` are reader
  commands and must consult it (see ROADMAP slice 19 row: "shared
  internal preflight used by 8 reader commands" — your two are in
  that 8).

---

## 7. Annotation discipline — read this before writing any function

CLAUDE.md §10 is **not** "annotate later." It is "annotate **as you
write**." The skill at `~/.claude/skills/aristo-authoring/SKILL.md`
(installed via `aristo install-skills`) is the authoring assistant —
use it. Do not hand-write `#[aristo::intent]` / `#[aristo::assume]`
attributes. The skill is the tool; the hand-written window closed at
slice 13.

When you root-cause a non-trivial bug during this work, encode the
root cause as a checkable intent + regression test. The repo's
`feedback_root_caused_bug_is_a_spec_case` philosophy says: trivial
bugs don't earn an intent; subtle bugs (especially after extended
debugging) do.

---

## 8. Definition of Done (from CLAUDE.md)

A slice is **done** only when ALL of:

1. ✅ Every promoted scenario for the slice is in `active/` and passing
2. ✅ Unit tests for new logic exist and pass
3. ✅ `cargo fmt --check`, `cargo check --workspace --all-targets`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace` all green
4. ✅ `CHANGELOG.md` `[Unreleased]` has one bullet per commit
5. ✅ All commits semantic + small (CLAUDE.md §1 / §2)
6. ✅ Branch pushed to origin
7. ✅ PR opened against `main` with a body that lists which scenarios
   were promoted (so the human can verify §12A discipline)

---

## 9. When to surface to the human (do not silently proceed)

Surface **before** acting, not after, if any of:

- A spec/mockup contradicts another (CLAUDE.md §9).
- A pending scenario can't be promoted byte-for-byte without changing
  the spec wording (CLAUDE.md §12 — never edit the spec to match the
  impl).
- You discover a "Phase 1 subset" of a feature that the mockup spec
  doesn't anticipate — surface the gap, get sign-off, document the
  carve-out, then build (CLAUDE.md §12 authorized-exception protocol).
- A conflict with Session A surfaces that isn't covered in §4 above.
- You'd need to refactor a path under §3 (Session A's exclusive zone)
  to land your slice cleanly.

---

## 10. Quick-reference paths

| What | Where |
|---|---|
| Roadmap | `docs/ROADMAP.md` |
| Working agreement | `CLAUDE.md` (root) |
| Testing convention | `docs/TESTING.md` |
| Decision archive (live, this repo) | `docs/decisions/` |
| Phase 0 design archive (parent repo) | `../aretta-sdk/docs/` via symlinks |
| Mockups for slice 28 | `../aretta-sdk/docs/mockups/10-doc-and-graph/` |
| Mockup for slice 31 | `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md` |
| Slice 28 pending scenarios | `crates/aristo-cli/tests/cmd/_pending/doc_*.md` |
| Slice 31 pending scenarios | `crates/aristo-cli/tests/cmd/_pending/stale_preflight_on_badge.md` |
| Closest module-shape analog (multi-file) | `crates/aristo-cli/src/commands/critique/` |
| Closest module-shape analog (single-file) | `crates/aristo-cli/src/commands/lang.rs` |
| Where the stubs currently live | `crates/aristo-cli/src/lib.rs` lines ~268–275 + ~365–367 |
| Re-point target on slice 28 landing | `crates/aristo-cli/tests/binary_smoke.rs` line 53 (`doc` → `graph`) |

---

## 11. Coordination needed

(Empty. Append entries here if you hit blockers that need Session A
or the human to act before you can proceed.)
