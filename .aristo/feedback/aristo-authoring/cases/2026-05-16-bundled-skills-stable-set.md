---
date: 2026-05-16
slice: 12
file: crates/aristo-cli/src/skills/mod.rs:28
id: bundled_skills_is_stable_set
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
---

## Original (v0)

> the bundled skills returned here are the authoritative set installed by aristo install-skills; renaming or removing one is a breaking change for any user who has the old name on disk and relies on agent matching

## Better (v2)

> Skill names in this set are part of the public install surface. Renaming or removing one is a breaking change — users on the old name have it on disk under that path; agents match by exact name.

## Why the gap

v0 leads with "the bundled skills returned here are the authoritative set installed by aristo install-skills" (narration of role). v2 leads with the implicit invariant: these names are part of the *public install surface* — same status as a public API name. The refactor-trap warning ("renaming = breaking") is preserved and tightened with the WHY (users have the old name on disk; agent matching is exact).

## Verify level

- was: `test`
- is: `neural`
- reason: per P-VERIFY-MATCHES-SHAPE, the load-bearing claim is "don't rename or remove" — a design / semver-level judgment, not a runtime property. A test could enumerate the set and require explicit acknowledgment-on-change (a snapshot test), but that's a process gate, not a runtime invariant. `neural` is more honest about what the intent is doing.

## Round-2 backfill note

Slices 10–13 backfill audit. Verify shift `test` → `neural`.
