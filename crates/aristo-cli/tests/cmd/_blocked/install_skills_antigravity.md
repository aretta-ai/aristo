# `aristo install-skills --agent=antigravity`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 5: Antigravity (file-copy model, exact format TBD)".

File-copy install model. Antigravity's exact skill file format is still stabilizing; the CLI emits a footer note inviting an issue if path or frontmatter doesn't match the user's Antigravity version.

```console
$ aristo install-skills --agent=antigravity

→ Installing Aristo skills for Antigravity …
  • Wrote: .antigravity/skills/aristo-authoring.md
  • Wrote: .antigravity/skills/aristo-neural-verify.md
  • Wrote: .antigravity/skills/aristo-mine-assertions.md
  • Wrote: .antigravity/skills/aristo-review-skill.md

ok: 4 skills installed for antigravity.
note: Antigravity's skill format is still stabilizing; if this file path
or frontmatter is wrong, please file an issue with the version of
Antigravity you're using.
```
