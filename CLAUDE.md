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

## §10. Annotation discipline — dogfood rule

From slice 6 onward (the moment `aristo-macros` ships), **we annotate Aristo's own source as we write it.** Aristo is its own first user; the friction we feel here is the friction every user will feel.

The rule is restrained, not exhaustive:

- `#[aristo::intent(...)]` and `#[aristo::assume(...)]` document **high-level invariants and properties** that, if violated, would cause silent correctness bugs. They are **not** a replacement for normal Rust doc comments — they **supplement** them.
- **Apply to:**
  - Functions whose correctness depends on a non-obvious invariant
  - Modules / structs / traits that uphold a system-wide property
  - Anywhere a regression would cause a silent correctness bug rather than a loud panic / type error
- **Don't apply to:**
  - Trivial getters / setters / `From` impls / `Display` impls
  - Pure data containers (a struct with three `pub` fields and no methods)
  - Anywhere the function name + signature already says everything
- **Aggressive when in doubt.** We can always relax the rule later; we cannot retroactively recover the experience of having lived with it.
- Once `aristo install-skills` is in place (slice 13), the **authoring skill writes intents — don't hand-write them.** Hand-writing exists only in the window between slices 6 and 13, deliberately short so we feel the gap the skill closes. The backfill commit at the start of milestone B sweeps slices 1–5 schema crates using the skill (don't hand-write the backfill — that's part of the skill test).

## §10A. Skill-feedback loop — PHILOSOPHY.md + cases (active development; design dated 2026-05-16)

**Status:** designed but not yet wired (lands in milestone D, see ROADMAP). Listed here so the design survives any context reset before the implementation lands.

The authoring skill (and future skills: mining, neural-verify, review) is taste-driven and we're bootstrapping that taste from scratch. The system has TWO living artifacts per skill:

- **`.aristo/feedback/<skill>/PHILOSOPHY.md`** — distilled principles, P-tagged (e.g. `P-VERIFY-OFF-WHEN-COVERED`), each with one-line rule + rationale paragraph + linked example cases. Modeled on Rust API Guidelines / OpenAI Model Spec / Chicago Manual of Style. Anti-patterns first; principles can be drafts; user can write principles directly without waiting for cases. **This is the only feedback artifact the skill itself loads** (alongside its SKILL.md body, via `include_str!`).
- **`.aristo/feedback/<skill>/cases/<date>-<slug>.md`** — audit-trail evidence per feedback round: original (skill-generated), better (per user), why, candidate_principle. Each case file stays as historical record + regression eval; the principle distilled from it lives in PHILOSOPHY.md.

**REFLECTION protocol** (triggered at milestone close + on-demand):
1. Read recent open cases for the skill.
2. Group by candidate_principle (or new draft principle).
3. Refine PHILOSOPHY.md: add new P-, sharpen existing ones, retire stale.
4. For each case, verify it now maps to a principle. Mark superseded; keep file as regression evidence.
5. Commit the PHILOSOPHY.md diff with a message linking the case files it addresses.

**Frequency calibration:** start frequent (every milestone, possibly more often). Track diff size — when REFLECTION starts producing tiny diffs, the philosophy is stabilizing and cadence drops to "as needed." Three reflections in we'll know the shape.

**End-of-milestone-C action item:** surface every #[aristo::intent] added in slices 14–17 (hash + id + walk + cycle + index + stamp), capture user feedback on each as cases, draft initial PHILOSOPHY.md for `aristo-authoring`. This is task #35 in the session task list.

## §11. Release cadence — milestone version bumps

The roadmap (`docs/ROADMAP.md`) groups slices into milestones. Each milestone closes with a workspace version bump and a git tag — the project gains a real release cadence instead of one big-bang publish.

- **Last commit of every milestone:** `chore(release): v0.0.X` (or `v0.X.0` per the roadmap's MVP cutoff).
  - Bumps `[workspace.package] version` in the root `Cargo.toml`.
  - CHANGELOG: promote the `## [Unreleased]` block to `## [v0.0.X] — YYYY-MM-DD`, add a fresh empty `## [Unreleased]` heading above it.
  - Commit body lists the milestone's slices (so the tag message is self-contained for `git show v0.0.X`).
- **Tag immediately after** the release commit lands on `main`:
  - `git tag -a v0.0.X -m "Milestone X — <one-line summary>"`
  - `git push origin v0.0.X`
- **Versions are dense and small.** v0.0.2 → v0.0.3 → ... → v0.0.8 → v0.1.0 (MVP). Don't skip numbers; don't lump multiple milestones into one bump.
- Mid-milestone hotfixes — a fix landing in milestone E that must ship before E completes — get a patch bump (e.g. v0.0.5.1 while E is heading to v0.0.6). Rare; should be the exception, not the rule.

## §12. Specifications are the truth — never rewrite to match impl

**This rule is non-negotiable.** Caught and added 2026-05-16 after an audit showed ~60% of scenario promotions (slices 10–19) had silently rewritten the pre-written spec to fit Phase-1 implementation reality. The spec disappeared from the repo in the major-rewrite cases; the test suite became a tautology.

The pre-written scenarios in `crates/aristo-cli/tests/cmd/_pending/`, the mockups in `../aretta-sdk/docs/mockups/`, and the decisions in `../aretta-sdk/docs/DECISIONS.md` + `../aretta-sdk/docs/TOOLS.md` are **THE SPEC**. They are the source of truth for what every CLI command does. They are NOT editable to match what the implementation happens to do today.

**Promotion `_pending/` → `active/` is byte-for-byte.** The only allowed alterations are trycmd wildcards (`[..]`, `[EXE]`) for fields that legitimately vary (timestamps, byte counts, file paths). Wildcards are NOT a license to paper over output-format differences.

**When implementation diverges from spec:**

- **DO close the gap in the implementation.**
- **DO NOT** silently adjust the scenario, mockup, or design doc to match.
- **DO NOT** ship an adapted "Phase 1 subset" version under the original filename — that mutates the spec out of existence.
- **DO NOT** delete a `_pending/` scenario because "trycmd can't easily express it" — the spec stays put; if needed, the fixture (`.in/`) does the expressing.
- **ANY** exception requires explicit human signoff. Raise the conflict; document the agreed change in this CLAUDE.md or the design archive; then update the spec. The sequence is: raise → sign off → amend spec → implement.

**Authorized prior exception:** the `intent!()` → `intent_stmt!()` rename in slice 11 was a Rust E0428 force (function-like and attribute proc-macros can't share a name). User accepted; design archive updated; this remains the *only* such authorized change.

**If a scenario can't be promoted yet** (Phase 2 server dependency, hard architectural blocker), it stays in `_pending/` with its content preserved verbatim. Defer scenarios, never spec content.

**Failure modes this rule prevents:**

- The spec gradually mutates to match each successive impl, ending in nothing being checked.
- Implementation gaps get hidden as "Phase 1 subset" annotations that nobody comes back to.
- Future contributors lose the design intent because the only place it was recorded (the active scenarios) was rewritten away.
- The test suite stops being a fidelity check and becomes "the code does what the code does."

**Operational test:** when about to edit a scenario file, mockup, or design doc to match what the impl produces, STOP. The discipline says fix the impl, not the spec. If the impl can't be fixed in scope, surface the conflict.

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
