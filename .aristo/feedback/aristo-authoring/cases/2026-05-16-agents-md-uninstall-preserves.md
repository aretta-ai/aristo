---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/skills/install.rs:131
id: agents_md_uninstall_preserves_outside_markers
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> agents_md_uninstall strips ONLY the marker-delimited block; the surrounding content is preserved byte-for-byte

## Better (v2)

> Only the marker-delimited block is stripped; surrounding content is preserved byte-for-byte. Absent file or absent block is not an error — idempotent.

## Why the gap

Light rewrite. v0 already states the byte-for-byte preservation invariant clearly. v2 inverts the lead ("Only the marker-delimited block is stripped" comes first, since that's the focused mutation), then explicitly captures the idempotence-on-absent property as the second sentence — v0 missed this entirely even though the impl returns `Ok(false)` on absent file or block.

## Verify level

- was: `test`
- is: `test`
- reason: both claims directly testable. Existing tests `agents_md_uninstall_strips_block_preserves_surrounding` and `agents_md_uninstall_idempotent_when_file_absent_or_block_absent` cover them.

## Round-2 backfill note

Slices 10–13 backfill audit. v2 surfaces an invariant v0 missed (idempotence on absent).
