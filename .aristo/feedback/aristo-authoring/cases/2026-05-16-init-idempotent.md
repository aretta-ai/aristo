---
date: 2026-05-16
slice: 10
file: crates/aristo-cli/src/commands/init.rs:46
id: init_is_idempotent
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> aristo init is idempotent: a second invocation never errors and never overwrites existing files; it notes the existing artifact and continues

## Better (v2)

> A second invocation never errors and never overwrites existing files. Each pre-existing artifact is noted; only missing ones get created.

## Why the gap

Mild: v0 leads with the function name + the property name ("aristo init is idempotent") instead of the invariant itself. v2 leads with the invariant ("a second invocation never errors and never overwrites"), which is what the reader / agent needs to know. Behavior-after-existing-artifact ("each pre-existing artifact is noted") is a separate testable property — kept.

## Verify level

- was: `test`
- is: `test`
- reason: directly testable as `init();init()` → assert second is no-op + no overwrite. Existing imperative tests cover this (`tests/init_command.rs`).

## Round-2 backfill note

Slices 10–13 shipped before the §10A reflection loop was established. This case is part of the backfill audit covering the 9 untouched intents from those slices.
