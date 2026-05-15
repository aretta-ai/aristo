# CLAUDE.md — Aristo working agreement

**This file is law.** Every Claude Code session in this repo MUST read this file before touching code, and MUST obey every rule below. These rules are not aspirational — they are how we work. Violating any of them is grounds for the user to revert your change.

If you find yourself rationalizing an exception ("just this once," "the rule doesn't really apply here"), STOP. The rule applies. If the rule is genuinely wrong, surface that to the user and propose a CLAUDE.md edit — do not silently bypass it.

---

## §1. Commit size — small or medium ONLY

- **Large commits are FORBIDDEN.** No exceptions.
- Heuristic: if the diff exceeds ~200 changed lines OR touches more than ~5 files for unrelated reasons, SPLIT IT.
- One logical change per commit. If the message needs the word "and," it is two commits.
- The only allowed "wide" commit is a mechanical, atomic refactor (e.g. a project-wide rename) that is trivially reviewable as a single operation. Surface these to the user before making them.

## §2. Commit messages — semantic / conventional

Required prefix from this exact set:

| Prefix | Use for |
|---|---|
| `feat:` | new user-visible functionality |
| `fix:` | bug fix |
| `refactor:` | code change that neither fixes a bug nor adds a feature |
| `perf:` | performance improvement, no behavior change |
| `docs:` | documentation only (including this file, README, CHANGELOG-only edits — but see §3) |
| `test:` | tests only |
| `build:` | build system, dependencies, `Cargo.toml`, workspace config |
| `chore:` | housekeeping that doesn't fit above |
| `ci:` | CI / GitHub Actions config |

Optional scope in parens: `feat(macros): ...`, `fix(cli): ...`, `build(workspace): ...`.

**Banned messages:** `wip`, `stuff`, `updates`, `misc`, `fixes`, `progress`, `more changes`. Say what changed.

## §3. CHANGELOG.md — one line per commit, in the same commit

- **Every commit MUST add at least one bullet** to the `## [Unreleased]` section of `CHANGELOG.md`, describing what changed in customer-facing language.
- The CHANGELOG bullet ships **in the same commit** as the code change. Never a separate "update changelog" commit.
- Format: `- <area>: <what changed and why a user cares>`. Examples:
  - `- macros: \`#[aristo::intent]\` now accepts multi-line text without escaping.`
  - `- cli: \`aristo stamp --check\` exits non-zero on staleness for CI gating.`
- At release: promote `## [Unreleased]` to `## [vX.Y.Z] — YYYY-MM-DD`. The `[Unreleased]` block must read coherently as a release-note draft when scanned end-to-end.

## §4. Test-first — no test, no claim of correctness

- **Write the test BEFORE the implementation**, as far as possible. Goal: surface ambiguity. If you cannot write the test, you do not yet know what you are building — go clarify before writing code.
- **NO TEST = NO CLAIM OF CORRECTNESS.** "Should work," "looks right," "compiles," "I checked it manually" are all NOT correctness. The bar is: a test demonstrates the behavior, the test passes, the test is committed alongside the code.
- The TDD inner loop is local; what gets committed is always green:
  1. Write failing test → run it → confirm it fails for the right reason.
  2. Write implementation → run test → it passes.
  3. Run the full check suite (§6).
  4. Commit (test + impl + CHANGELOG bullet, all together).

## §5. Autonomous diagnosis when coverage is good

- When the area you are touching has good test coverage, **diagnose and fix problems autonomously**. Read the failure, form a hypothesis, test it, iterate. Do not punt to the human after a single failure — that is what the test suite is for.
- When coverage is thin and behavior is ambiguous, **stop and surface the ambiguity** to the user before guessing. Better to ask than to encode the wrong invariant.

## §6. Every commit passes ALL checks

Before `git commit`, you MUST run and pass:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- All four MUST be green. If clippy fires a warning, fix the root cause — do NOT add `#[allow(...)]` without first asking the user whether the lint should be suppressed for this case.
- If a pre-commit hook fails, FIX THE ROOT CAUSE. Never use `--no-verify`. Never use `--no-gpg-sign` unless the user explicitly asks for it.
- A failed pre-commit hook means the commit DID NOT happen. After fixing, create a NEW commit — never `--amend`, because `--amend` would modify the previous commit.

## §7. No hacky fixes — refactor before patching

- Architect for **maximal code reuse**. If you find yourself copy-pasting, factor.
- If a clean fix demands a refactor that touches files outside the immediate change, **STOP and surface it to the user** for a decision before proceeding. Do not silently widen the blast radius of a small change.
- "Hacky" includes: special-casing a single caller instead of fixing the abstraction; adding a flag to opt out of a buggy behavior instead of fixing it; copy-pasting a function and tweaking one line.

## §8. Small batches — ship fast, ship incomplete, ship green

- Cycle: **choose task → write test → implement → pass test → commit → push**. End every cycle with a green build pushed to origin.
- It is OK — and expected — to ship an **incomplete feature** with a clearly-labeled `unimplemented!("<what's missing and why>")`. Better than waiting until the whole feature is done.
  - Every `unimplemented!()` MUST have an explanatory message. Bare `unimplemented!()` is forbidden.
  - Every `todo!()` and `unimplemented!()` is tracked: when introducing one, add a CHANGELOG bullet noting the gap and a corresponding entry in `docs/ROADMAP.md` if the gap is non-trivial.
- Do NOT accumulate multiple half-done features in the working tree. Finish one slice (or land it with explicit `unimplemented!()` markers) before starting another.

## §9. Plan-driven — work from `docs/ROADMAP.md`

- All work is governed by a top-level plan. The active plan lives in `docs/ROADMAP.md` (or, for a specific phase, `docs/PHASE-N-PLAN.md`).
- **Before picking a task, check the plan** to ensure it is the next-most-valuable work — avoid local minima driven by what is currently on screen.
- If the plan is silent on what to do next, **surface that gap** to the user and propose a plan update before coding speculatively.
- The Phase 0 design archive (decisions, surface, mockups) lives in the parent repo at `../docs/`. When in doubt about behavior, the design archive overrides intuition. Authoritative files:
  - `../docs/DECISIONS.md` — every locked design decision with rationale.
  - `../docs/TOOLS.md` — current surface (commands, macros, config).
  - `../docs/mockups/12-phase-1-architecture/` — workspace + skills layout this repo implements.

---

## Definition of Done

A change is "done" only when ALL of the following hold:

1. ✅ A test demonstrates the new or changed behavior, AND that test passes.
2. ✅ `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
3. ✅ `CHANGELOG.md` `[Unreleased]` has a one-line entry describing the change.
4. ✅ The commit is semantic (per §2) and small or medium-sized (per §1).
5. ✅ The commit is pushed to `origin`.

Skipping ANY of these = NOT done. Do NOT claim completion to the user until all five hold.

---

## Anti-patterns — DO NOT DO THESE

- ❌ "I'll add the test after I see if this works."
- ❌ "It compiles, so it should be correct."
- ❌ "Let me bundle these three changes into one commit."
- ❌ Updating `CHANGELOG.md` in a separate commit from the code.
- ❌ Suppressing a clippy warning instead of fixing it.
- ❌ Adding speculative abstractions, config knobs, or plugin points before a second concrete case demands them.
- ❌ Bare `unimplemented!()` or `todo!()` with no message.
- ❌ Patching around a design problem instead of fixing it.
- ❌ Force-pushing without explicit user permission.
- ❌ Using `git commit --amend` to fix a failed pre-commit hook (the original commit didn't happen — make a NEW commit).
- ❌ Marking work "done" with any of the §6 checks unrun or any of the Definition of Done items unchecked.

---

## When in doubt

Ask the user. The cost of a clarifying question is low; the cost of encoding the wrong invariant is high. The exceptions to "ask the user" are §5 (autonomous diagnosis on well-covered code) and the user explicitly telling you to work without stopping for clarifications — in which case make the reasonable call and continue, but flag any decision you'd normally have asked about.
