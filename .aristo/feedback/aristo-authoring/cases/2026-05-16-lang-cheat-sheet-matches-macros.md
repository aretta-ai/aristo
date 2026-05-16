---
date: 2026-05-16
slice: 11
file: crates/aristo-cli/src/commands/lang.rs:57
id: lang_cheat_sheet_matches_macros
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP]
verify_was: test
verify_is: test
---

## Original (v0)

> aristo lang's output is the single source of truth for annotation syntax that authoring skills consult; the cheat sheet must always match the macros aristo-macros currently exports

## Better (v2)

> The cheat sheet text MUST match the macros `aristo-macros` currently exports. Adding, renaming, or removing a macro requires updating the cheat sheet in the same change — agents are instructed to trust this output over their training data.

## Why the gap

v0 starts with two clauses of narration ("aristo lang's output is the single source of truth", "authoring skills consult") before reaching the load-bearing invariant ("must always match"). v2 leads with the invariant + makes the refactor trap explicit ("adding, renaming, or removing a macro requires updating the cheat sheet in the same change") so a future macro rename can't slip through silently. The "agents trust this over training data" clause is preserved as the reason this invariant matters.

## Verify level

- was: `test`
- is: `test`
- reason: existing regression guard `cheat_sheet_uses_intent_stmt_not_intent_bang` in `crates/aristo-cli/src/skills/mod.rs` is the canonical form; broader cheat-sheet-vs-macro-export coverage is a small extension.

## Round-2 backfill note

Slices 10–13 backfill audit. No verify shift.
