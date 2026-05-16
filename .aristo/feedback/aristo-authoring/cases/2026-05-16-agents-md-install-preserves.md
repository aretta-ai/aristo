---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/skills/install.rs:84
id: agents_md_install_preserves_outside_markers
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-WHY-AS-INVARIANT]
verify_was: test
verify_is: test
---

## Original (v0)

> agents_md_install preserves all content outside the marker boundaries verbatim; this is what lets users hand-edit AGENTS.md without losing their work on aristo install-skills --update

## Better (v2)

> Content outside the marker boundaries is preserved byte-for-byte across install and update. Users who hand-edit AGENTS.md alongside the auto-generated block don't lose their work to a normalization or reformat pass.

## Why the gap

v0 has good content already. v2 tightens by leading with the invariant (rather than the function name) and naming the specific refactor traps ("normalization or reformat pass") — the kinds of cleanup PRs that would silently violate the byte-for-byte guarantee.

Per P-WHY-AS-INVARIANT: the "users who hand-edit don't lose work" clause IS the design rationale, not motivation prose — it's the WHY a refactor proposer would have to argue against. Kept.

## Verify level

- was: `test`
- is: `test`
- reason: byte-for-byte preservation is directly testable (write fixture AGENTS.md with hand-written content above/below markers; install/update; assert non-marker bytes unchanged). Existing tests `agents_md_appends_block_to_existing_file_preserving_user_content` and `agents_md_replaces_only_marker_block_on_update` cover it.

## Round-2 backfill note

Slices 10–13 backfill audit.
