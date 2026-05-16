---
date: 2026-05-15
slice: 17
file: crates/aristo-cli/src/commands/stamp.rs:36
id: stamp_check_never_writes
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP]
verify_was: test
verify_is: test
---

## Original (v0)

> aristo stamp NEVER writes the index when --check is on; --check is the CI-safe inspection path. The implementation MUST gate the atomic_write call on the !check branch — a regression here would cause CI runs to mutate the index, masking real drift in PRs.

## Better (v2)

> When `--check` is set, `aristo stamp` never writes the index. CI relies on this for drift detection: a regression that mutates the index under `--check` would silently mask the drift it was meant to catch.

## Why the gap

v0 has implementation-direction prose ("The implementation MUST gate the atomic_write call on the !check branch") and category narration ("--check is the CI-safe inspection path"). v2 states the user-visible contract and names the refactor trap with its consequence (P-NAME-THE-REFACTOR-TRAP): mutating under `--check` would *silently* mask the drift it was meant to catch — that's the disaster scenario the intent guards against.

## Verify level

- was: `test`
- is: `test`
- reason: directly testable as "file mtime unchanged after `--check`" — exactly what the existing tests `check_mode_does_not_write_when_index_matches` and `check_mode_exits_nonzero_when_index_is_stale` check.
