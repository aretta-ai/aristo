---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/skills/install.rs:41
id: file_copy_install_idempotent
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NO-DOUBLE-INTENT]
verify_was: test
verify_is: test
---

## Original (v0)

> file_copy_install is idempotent: invoking it twice with identical content leaves the disk byte-identical the second time and reports Unchanged; a callers' --update message should distinguish Created vs Updated vs Unchanged correctly

## Better (v2)

> A second invocation with identical content leaves the target byte-identical and returns `Unchanged`. Created (file did not exist) and Updated (content differed) are distinct outcomes; idempotence is the Unchanged case specifically.

## Why the gap

P-NO-DOUBLE-INTENT split: v0 mixes this function's invariant (the tri-state outcome and idempotence-on-Unchanged) with a caller-side guideline ("a callers' --update message should distinguish ... correctly"). The caller-side prose is policy for downstream consumers, not a property of `file_copy_install` itself, and was dropped in v2 — callers (e.g., `commands::install_skills`) can choose how to surface the tri-state.

v2 also drops the "is idempotent:" framing because the property name doesn't add information beyond the rule that follows.

## Verify level

- was: `test`
- is: `test`
- reason: tri-state outcome is directly testable. Existing test `file_copy_creates_then_unchanged_then_updated` exercises all three outcomes.

## Round-2 backfill note

Slices 10–13 backfill audit.
