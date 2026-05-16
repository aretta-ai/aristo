---
date: 2026-05-15
slice: 14A
file: crates/aristo-core/src/hash.rs:38
id: body_hash_is_verbatim
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP]
verify_was: test
verify_is: test
---

## Original (v0)

> body_hash is verbatim — the covered region's bytes hash exactly as they appear in source. This is what makes stamp's drift detection work: any change inside the covered region changes the hash, flips status to `unknown`, and the author re-verifies. Whitespace-only edits ARE drift by design (the human reviewed THIS code, not other code; even cosmetic changes deserve a fresh look).

## Better (v2)

> Every byte inside the covered region is significant to the body hash. Identical hash means byte-identical region; any difference, including whitespace, is drift.

## Why the gap

v0 narrates the downstream consequence ("This is what makes stamp's drift detection work…flips status to unknown") instead of stating the invariant. The parenthetical justification is motivation, not spec. v2 states the bijection between byte equality and hash equality, naming "even whitespace" as the implicit invariant a refactor might break (P-NAME-THE-REFACTOR-TRAP — a "consistency cleanup" PR would propose normalizing whitespace here too, like `text_hash` does).

## Verify level

- was: `test`
- is: `test`
- reason: bijection between byte equality and hash equality is testable as `body_hash(a) == body_hash(b) iff a == b`.
