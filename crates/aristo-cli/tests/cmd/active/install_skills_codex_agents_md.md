# `aristo install-skills --agent=codex` — AGENTS.md section injection

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Agent 3: Codex (AGENTS.md section-injection model)".

The AGENTS.md install model (per K4): instead of writing per-skill files, inject a single marker-delimited block into the project's `AGENTS.md`. Hand-written content outside the markers is preserved across reinstalls / updates.

Block format: `<!-- ARISTO-SKILLS START v1 -->` … `<!-- ARISTO-SKILLS END -->`. The version pin in the start marker lets `--update` (slice 19+) detect drift and rewrite the block in place.

```console
$ aristo install-skills --agent=codex

→ Installing Aristo skills for Codex (project-level) …
  • Created: AGENTS.md

ok: 1 skill installed for codex.

```
