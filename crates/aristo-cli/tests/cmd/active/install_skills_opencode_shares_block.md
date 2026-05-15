# `aristo install-skills --agent=opencode`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 4: OpenCode (AGENTS.md, shares block with codex)".

OpenCode reads the same `AGENTS.md` Codex does, with the same marker-delimited block format. Installing for opencode is equivalent to installing for codex — same on-disk effect.

```console
$ aristo install-skills --agent=opencode

→ Installing Aristo skills for OpenCode (project-level) …
  • Created: AGENTS.md

ok: 1 skill installed for opencode.

```
