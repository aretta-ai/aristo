---
date: 2026-05-15
slice: 14A
file: crates/aristo-core/src/id.rs:33
id: snake_case_from_text_returns_none_on_unusable_input
verdict: delete
principles: [P-INVARIANT-NOT-IMPL, P-INVARIANT-AT-LOAD-BEARING-SITE, P-NO-DOUBLE-INTENT]
verify_was: test
verify_is: (deleted)
---

## Original (v0)

> snake_case_from_text returns None if no usable readable id can be derived (text is empty, all non-ASCII, or contains no word characters). Callers MUST handle None by falling back to generate_opaque_id — that's the contract that lets stamp always produce a valid id even from pathological annotations.

## Better

(none — annotation deleted from source)

## Why deleted

Two distinct intents jammed into one (P-NO-DOUBLE-INTENT):

1. **When this function returns None** — the predicate ("empty, all non-ASCII, or no word chars") is implementation detail; the signature `-> Option<AnnotationId>` already signals "may not produce one." Specifying the exact predicate over-constrains future implementations and restates what the type system says (P-INVARIANT-NOT-IMPL).

2. **"Stamp always produces a valid id"** — this IS load-bearing, but it's a system-wide invariant. It belongs at the *enforcement site* (`build_entries` in `commands/index.rs`, where the fallback ladder lives), not on one of three id sources (P-INVARIANT-AT-LOAD-BEARING-SITE).

After deleting here, the system-wide claim is preserved on `build_entries_assigns_opaque_ids_when_missing` (case [build-entries-id-assignment](./2026-05-15-build-entries-id-assignment.md)). Net effect: same coverage, less noise, invariant lives at the right site.

## Verify level

n/a (deleted).
