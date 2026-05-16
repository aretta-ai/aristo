---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/commands/install_skills.rs:64
id: install_skills_scope_symmetry
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
---

## Original (v0)

> install_skills emits the same observable user experience whether invoked at project (cwd) or user (~/) scope; the only difference is where files land

## Better (v2)

> The user-visible output (lines printed, progression, success summary) is identical at project scope (cwd) and user scope (`~/`). Only the target path differs.

## Why the gap

v0 says "same observable user experience" which is vague. v2 enumerates what "observable" means here (lines printed, progression, success summary) — concrete enough that a reader knows what to keep aligned across the two branches, generic enough that future polish to the install messages doesn't have to amend the intent.

## Verify level

- was: `test`
- is: `neural`
- reason: per P-VERIFY-MATCHES-SHAPE, the load-bearing claim is a qualitative UX-parity judgment ("same user-visible output across two branches"). Specific output strings ARE testable (spot-check assertions), but the broader "stays in parity over time" claim is a design judgment best verified by code review. Future install-message refactors should preserve the parity intentionally; that's a `neural` claim.

## Round-2 backfill note

Slices 10–13 backfill audit. Verify shift `test` → `neural`.
