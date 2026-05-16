# `aristo install-skills --agent=cursor`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 2: Cursor (file-copy model, project-level)".

File-copy install model: writes one `.mdc` per skill to `.cursor/rules/<skill>.mdc`. Same project-level-by-default policy as Claude Code; matches Cursor's native rules-file convention.

```console
$ aristo install-skills --agent=cursor

→ Installing Aristo skills for Cursor (project-level) …
  • Wrote: .cursor/rules/aristo-authoring.mdc
  • Wrote: .cursor/rules/aristo-neural-verify.mdc
  • Wrote: .cursor/rules/aristo-mine-assertions.mdc
  • Wrote: .cursor/rules/aristo-review-skill.mdc

ok: 4 skills installed for cursor.

Tip: commit .cursor/ to share rules with your team. To install globally
instead, pass --user (writes to ~/.cursor/rules/).
```
