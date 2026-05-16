# `aristo install-skills --agent=opencode` — reuses Codex AGENTS.md block

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 4: OpenCode (same AGENTS.md convention as Codex)".

Codex and OpenCode share the AGENTS.md convention. When a marker-delimited Aristo block already exists in the project's `AGENTS.md`, installing for OpenCode is a no-op-write — the block is reused. Running `install-skills` for either agent is idempotent.

This scenario assumes `install-skills --agent=codex` ran first (or vice versa). The fixture for the active form should establish that precondition.

```console
$ aristo install-skills --agent=opencode

→ Installing Aristo skills for OpenCode …
  • AGENTS.md found at [..]/AGENTS.md
  • Reusing marker-delimited Aristo section already established by codex

ok: 4 skills installed for opencode.

Note: Codex and OpenCode share the AGENTS.md convention; both agents read
from the same marker-delimited block. Running `aristo install-skills` for
either is idempotent.

```
