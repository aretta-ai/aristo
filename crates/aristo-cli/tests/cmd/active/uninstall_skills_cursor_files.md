# `aristo uninstall-skills --agent=cursor`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Uninstall (file-copy agents)".

Reverses the file-copy install: removes each skill's `.mdc` from `.cursor/rules/`. The directory itself isn't touched (it may contain user-authored rules). Idempotent: a second invocation reports nothing to do.

The trycmd sandbox here installs and then uninstalls within a single test, so the skill is present when uninstall runs.

```console
$ aristo install-skills --agent=cursor
...

$ aristo uninstall-skills --agent=cursor

→ Uninstalling Aristo skills for Cursor (project-level) …
  • Removed: .cursor/rules/aristo-authoring.mdc

ok: 1 skill removed for cursor.

$ aristo uninstall-skills --agent=cursor

→ Uninstalling Aristo skills for Cursor (project-level) …

ok: nothing to do (no Aristo skills installed for cursor).

```
