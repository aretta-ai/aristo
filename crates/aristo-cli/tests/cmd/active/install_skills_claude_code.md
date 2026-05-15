# `aristo install-skills --agent=claude-code`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 1: Claude Code (file-copy model, project-level default)".

File-copy install model (per K4): writes one `SKILL.md` per skill to `.claude/skills/<skill>/SKILL.md`. Project-level by default so `git clone` propagates skills to teammates without re-running the install. Skill content is version-pinned to the SDK build.

Slice 12 ships only the authoring skill; `aristo-mining`, `aristo-neural-verify`, and `aristo-review-skill` get bundled in their consuming slices (24, 23, 27) — at which point this scenario adds the corresponding `• Wrote:` lines.

```console
$ aristo install-skills --agent=claude-code

→ Installing Aristo skills for Claude Code (project-level) …
  • Wrote: .claude/skills/aristo-authoring/SKILL.md

ok: 1 skill installed for claude-code.

Tip: commit .claude/ to share skills with your team. To install globally
instead, pass --user (writes to ~/.claude/skills/).

```
