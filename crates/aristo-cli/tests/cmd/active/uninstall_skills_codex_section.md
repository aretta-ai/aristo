# `aristo uninstall-skills --agent=codex` — strip the AGENTS.md block

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Uninstall (AGENTS.md agents)".

Reverses the AGENTS.md install: strips the marker-delimited block, preserving everything outside the markers byte-for-byte (so user-authored content survives uninstall).

```console
$ aristo install-skills --agent=codex
...

$ aristo uninstall-skills --agent=codex

→ Uninstalling Aristo skills for Codex (project-level) …
  • Stripped Aristo block from: AGENTS.md

ok: 1 skill removed for codex.

$ aristo uninstall-skills --agent=codex

→ Uninstalling Aristo skills for Codex (project-level) …

ok: nothing to do (no Aristo skills installed for codex).

```
