# `aristo install-skills --list-agents`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Listing supported agents".

Enumerates supported agents and their install models. Initial Phase 1 set: `claude-code`, `cursor`, `codex`, `opencode`, `antigravity`. Project-level scope is the default; `--user` switches to user-level dirs. Adding agent N (per K4 `Agent` trait) extends this list automatically.

```console
$ aristo install-skills --list-agents
Supported agents:
  • claude-code   — file copy to .claude/skills/<skill>/SKILL.md
  • cursor        — file copy to .cursor/rules/<skill>.mdc
  • codex         — AGENTS.md section injection
  • opencode      — AGENTS.md section injection (shares block with codex)
  • antigravity   — file copy to .antigravity/skills/<skill>.md (format TBD)

Default install scope: project-level. Pass --user to install at user level
(skills available across all projects on this machine).

```
