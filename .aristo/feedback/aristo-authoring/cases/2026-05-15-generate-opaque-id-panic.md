---
date: 2026-05-15
slice: 14A
file: crates/aristo-core/src/id.rs:74
id: generate_opaque_id_panics_on_rng_failure
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-WHY-AS-INVARIANT, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
---

## Original (v0)

> generate_opaque_id always returns a parseable AnnotationId with the `aret_` prefix. The OS RNG (getrandom) is the source of entropy; if it fails (extremely rare — usually a misconfigured kernel), this function panics rather than returning a Result. The reasoning: a stamped id with weak entropy is worse than a crashed `aristo stamp` run that the user can retry.

## Better (v2)

> Opaque ids carry enough entropy that collisions across a project are negligible. If the OS can't produce randomness, the stamp crashes; a low-entropy id silently committed would be worse than a failed run the user can retry.

## Why the gap

v0 has implementation commentary ("The OS RNG (getrandom) is the source", "extremely rare — usually a misconfigured kernel") and an explicit meta-narrative ("The reasoning:"). v2 keeps the *design choice* ("a low-entropy id silently committed would be worse") because that's the load-bearing implicit invariant (P-WHY-AS-INVARIANT) — a reviewer proposing "return Result for good error handling" would silently change the failure semantics. The "why" content here IS the spec.

## Verify level

- was: `test`
- is: `neural`
- reason: load-bearing claim is "panic is the right failure mode (vs. Result)" — a design judgment. Sub-claims (entropy bit count, aret_ prefix) are testable but secondary; per P-VERIFY-MATCHES-SHAPE the verify level should track the *load-bearing* claim's shape. An LLM reading the code can verify the design choice; a mined runtime assertion cannot.
