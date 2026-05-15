# `aristo install-skills --agent=cursor`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 2: Cursor (file-copy model, project-level)".

File-copy install model: writes one `.mdc` per skill to `.cursor/rules/<skill>.mdc`. Same project-level-by-default policy as Claude Code; matches Cursor's native rules-file convention.

Slice 12 ships only the authoring skill; the other three skills get bundled in slices 23 / 24 / 27.

```console
$ aristo install-skills --agent=cursor

→ Installing Aristo skills for Cursor (project-level) …
  • Wrote: .cursor/rules/aristo-authoring.mdc

ok: 1 skill installed for cursor.

Tip: commit .cursor/ to share skills with your team. To install globally
instead, pass --user (writes to ~/.cursor/rules/).

```
