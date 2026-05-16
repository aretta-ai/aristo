---
date: 2026-05-15
slice: 14B
file: crates/aristo-core/src/walk/extract.rs:84
id: extract_returns_annotations_in_source_order
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP]
verify_was: test
verify_is: test
---

## Original (v0)

> extract_from_source returns annotations in source order (top of file first). Tests rely on this ordering to assert specific entries by index without selector machinery, and the downstream walker depends on it for stable index.toml ordering when ids haven't been assigned.

## Better (v2)

> Annotations return in source order — top of file first. Sorting or hashing the result would silently break stable index ordering and the test fixtures that index into it positionally.

## Why the gap

v0 narrates the *callers* ("Tests rely on…downstream walker depends on…"). v2 states the invariant first, then names the specific refactor that'd break it (P-NAME-THE-REFACTOR-TRAP: "sorting or hashing the result" — exactly the kind of "let's use a HashMap for O(1) lookups" cleanup an agent would propose). The caller-list is less useful than naming the trap.

## Verify level

- was: `test`
- is: `test`
- reason: source-order property is directly testable (multi-annotation source, assert extracted order matches source order).
