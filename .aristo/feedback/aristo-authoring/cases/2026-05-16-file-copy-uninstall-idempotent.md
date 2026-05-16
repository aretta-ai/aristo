---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/skills/install.rs:67
id: file_copy_uninstall_idempotent
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> file_copy_uninstall removes ONLY the file we wrote; absence of the file is not an error (idempotent uninstall)

## Better (v2)

> Removes only the file we wrote — no sibling deletion, no parent-dir cleanup. Absence of the target is not an error; uninstall-of-already-uninstalled is the idempotent case.

## Why the gap

v0 conveys two invariants in a single sentence: (a) safety scope (don't delete neighbors), (b) idempotence on absent target. v2 keeps them in one annotation (same function, same domain layer) but tightens both: "no sibling deletion, no parent-dir cleanup" names two specific refactor traps (a future "cleanup empty parent" optimization, a "wildcard the directory" change). The "idempotent uninstall" parenthetical becomes the second sentence.

## Verify level

- was: `test`
- is: `test`
- reason: both claims testable. Existing test `file_copy_uninstall_idempotent` covers the absent-target case.

## Round-2 backfill note

Slices 10–13 backfill audit.
