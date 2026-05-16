# `aristo uninstall-skills --agent=cursor` — file removal, skip locally modified

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "`aristo uninstall-skills` mirrors the install model" (cursor case).

For file-copy agents, uninstall removes each per-skill file. Files that have been locally edited since install (content hash differs from the embedded SDK version) are skipped with an advisory; pass `--force` to delete anyway.

```console
$ aristo uninstall-skills --agent=cursor

→ Removing Aristo skills for Cursor …
  • Removed: .cursor/rules/aristo-authoring.mdc
  • Removed: .cursor/rules/aristo-neural-verify.mdc
  • Removed: .cursor/rules/aristo-mine-assertions.mdc
  • Skipped: .cursor/rules/aristo-review-skill.mdc (locally modified; pass --force)

ok: 3 skills removed, 1 skipped.

```
