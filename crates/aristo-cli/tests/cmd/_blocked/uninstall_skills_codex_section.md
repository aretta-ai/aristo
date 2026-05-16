# `aristo uninstall-skills --agent=codex` — strip AGENTS.md section

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "`aristo uninstall-skills` mirrors the install model" (codex case).

For agents that use the AGENTS.md section-injection model (Codex, OpenCode), uninstall removes the marker-delimited Aristo block in place; hand-written content outside the markers is preserved.

```console
$ aristo uninstall-skills --agent=codex

→ Removing Aristo skills for Codex …
  • Stripped marker-delimited Aristo section from AGENTS.md (lines [..] of [..]).
  • Hand-written AGENTS.md content preserved.

ok: skills removed.

```
