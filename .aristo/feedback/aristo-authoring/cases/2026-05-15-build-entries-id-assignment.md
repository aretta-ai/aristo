---
date: 2026-05-15
slice: 16
file: crates/aristo-cli/src/commands/index.rs:103
id: build_entries_assigns_opaque_ids_when_missing
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-INVARIANT-AT-LOAD-BEARING-SITE]
verify_was: test
verify_is: test
---

## Original (v0)

> build_entries assigns an opaque aret_<random> id to every annotation that lacks a user-written id; aristo stamp (slice 17) offers to promote opaque ids to readable ones via the rename flow. The IndexFile schema requires every entry to have an id — there is no `unindexed` half-state.

## Better (v2)

> Every discovered annotation gets an id, sourced in this order: user-written `id =`, then a snake_case slug derived from the text, then a random `aret_…` opaque id. The build never returns an entry without an id; there is no "unindexed" half-state.

## Why the gap

v0 mixes the function's invariant with downstream narration ("aristo stamp offers to promote opaque ids to readable ones") and partly restates the schema. v2 focuses on the one load-bearing claim *at this enforcement site* (P-INVARIANT-AT-LOAD-BEARING-SITE): every annotation gets an id, sourced via this three-step ladder. The "no unindexed half-state" framing is the refactor-trap warning (don't return entries with `None` id "for cases where text is unhelpful").

This is also the new home for the system-wide "every annotation gets an id" claim that was previously misplaced on `snake_case_from_text` (see case [snake-case-from-text-delete](./2026-05-15-snake-case-from-text-delete.md)).

## Verify level

- was: `test`
- is: `test`
- reason: directly testable — feed annotations with/without user-written ids, assert all results have ids.
