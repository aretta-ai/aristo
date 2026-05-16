---
date: 2026-05-15
slice: 14A
file: crates/aristo-core/src/index/strings.rs:155
id: sha256_from_bytes_is_canonical_form
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
weak_pass: true
---

## Original (v0)

> Sha256::from_bytes is the only path that produces a Sha256 from raw input — every other constructor (parse) validates a pre-formatted string. Computing the digest here, in one place, guarantees the output is the canonical `sha256:<64-lowercase-hex>` form that round-trips through parse() and matches the schema pattern.

## Better (v2)

> A hash computed by this constructor is always in canonical form — the same form `parse` accepts and the same form written to the index file. Hashes never need re-validation after computation.

## Why the gap

v0 narrates the API shape ("is the only path that produces…every other constructor…") and pins the canonical form to a regex pattern. v2 uses the concrete noun "canonical form" instead of the regex, which is more readable AND less brittle (changing the regex doesn't rot the intent).

Marked weak-pass under the content gate: the round-trip property is real but thin — adding a non-canonical-emitting constructor in the future is the only refactor that'd break it, and that's not a likely refactor. Kept as a module-level invariant ("all Sha256 constructors emit canonical form") but flagged as a candidate for future cleanup if it becomes noise.

## Verify level

- was: `test`
- is: `test`
- reason: round-trip property `Sha256::parse(Sha256::from_bytes(b).as_str()).is_ok()` is directly testable.
