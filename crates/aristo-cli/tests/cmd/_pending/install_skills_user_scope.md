# `aristo install-skills --user` — cross-project install at user level

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → User-level (cross-project) install".

The `--user` flag opts out of the project-level default and writes skills to user-level dirs (e.g., `~/.claude/skills/`, `~/.cursor/rules/`). They apply across every project on the machine but may drift relative to project-pinned SDK versions; the CLI emits an advisory note about this trade-off.

```console
$ aristo install-skills --agent=claude-code --user

→ Installing Aristo skills for Claude Code (user-level) …
  • Wrote: ~/.claude/skills/aristo-authoring/SKILL.md
  • Wrote: ~/.claude/skills/aristo-neural-verify/SKILL.md
  • Wrote: ~/.claude/skills/aristo-mine-assertions/SKILL.md
  • Wrote: ~/.claude/skills/aristo-review-skill/SKILL.md

ok: 4 skills installed for claude-code at user level.

Note: user-level skills apply to ALL projects on this machine. They may
become stale relative to project-pinned SDK versions; project-level
install (the default) keeps each project's skills aligned with that
project's aristo dependency.
```
