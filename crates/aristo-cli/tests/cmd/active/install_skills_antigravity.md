# `aristo install-skills --agent=antigravity`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 5: Antigravity (file-copy, format TBD)".

File-copy install model: writes one `.md` per skill to `.antigravity/skills/<skill>.md`. Antigravity's skill format is still stabilizing; this layout is our best-effort interpretation, and the path may change before v0.1.0.

```console
$ aristo install-skills --agent=antigravity

→ Installing Aristo skills for Antigravity (project-level) …
  • Wrote: .antigravity/skills/aristo-authoring.md

ok: 1 skill installed for antigravity.

Tip: commit .antigravity/ to share skills with your team. To install globally
instead, pass --user (writes to ~/.antigravity/skills/).

```
