---
date: 2026-05-15
slice: 16
file: crates/aristo-cli/src/commands/index.rs:291
id: atomic_write_via_tempfile_rename
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-WHY-AS-INVARIANT, P-NO-DOUBLE-INTENT, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
---

## Original (v0)

> atomic_write writes via temp file + rename in the same directory, so a crash leaves either the prior index or the new one — never a half-written file. The temp suffix `.tmp` is fixed (no PID / random component) so concurrent invocations of `aristo index` clash — that's the right behavior; running two indexers against the same workspace is a user error.

## Better (v2)

> A crash mid-write leaves either the prior file or the new file at the target — never a partial one. The temp file's suffix is fixed, not randomized, so two concurrent invocations clash on the temp file — intentional, since running two indexers against one workspace is a user error we surface loudly.

## Why the gap

Two distinct claims in one annotation: (a) atomicity, (b) deliberate non-randomized suffix as concurrent-clash guard. Both are load-bearing — atomicity is the function's primary purpose, the fixed-suffix choice is the non-obvious design decision a refactor would propose to reverse ("let's add PID for concurrent safety" would silently change semantics).

P-NO-DOUBLE-INTENT would normally suggest splitting, but exception applies: both claims share one function and are both about the file-system semantics of one write operation. Keeping them together preserves the "this function's two file-system contracts" framing.

v0 has narration ("so a crash leaves…", "so concurrent invocations…clash — that's the right behavior") and an explicit "is the right behavior" judgment. v2 keeps the design content because it's load-bearing (P-WHY-AS-INVARIANT) but tightens the framing.

## Verify level

- was: `test`
- is: `neural`
- reason: atomicity is awkward to mine into a runtime assertion (would require crashing the process mid-write and checking the file from outside). The concurrent-clash piece is a design judgment. Both verifiable by reading code; neither by mined test. Per P-VERIFY-MATCHES-SHAPE.
