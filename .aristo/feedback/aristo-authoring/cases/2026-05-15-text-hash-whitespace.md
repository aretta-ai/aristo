---
date: 2026-05-15
slice: 14A
file: crates/aristo-core/src/hash.rs:25
id: text_hash_normalizes_whitespace
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> text_hash normalizes whitespace before hashing so that lint-induced reformatting (re-wrapping a long string, fixing indentation) doesn't invalidate stamped annotations. The mapping is: trim ends, then collapse runs of ASCII whitespace into a single space.

## Better (v2)

> Whitespace differences in annotation text — leading, trailing, or runs collapsed to one space — do not change the text hash. Reformatting prose is not drift.

## Why the gap

v0 leads with motivation ("so that lint-induced reformatting…doesn't invalidate") and embeds implementation specifics in parens. v2 states the equivalence-class invariant directly. Same content, less filler. The "lint reformatting" framing is the *use case*, not the invariant — that belongs in commit history or a doc comment, not in the intent body.

## Verify level

- was: `test`
- is: `test`
- reason: equivalence-class property is directly mineable as a runtime assertion (`text_hash(s) == text_hash(reformat(s))` for various reformat operations).
