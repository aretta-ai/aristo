# `aristo install-skills --update` — no-op when already current

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K4 → Update an existing install".

`--update` re-installs the embedded SDK version of every skill, but skips writes when the on-disk content already matches the embedded version (skills are version-pinned to the SDK build per K4). This scenario captures the no-change case; the modified-content case (with conflict prompt) is a separate scenario.

```console
$ aristo install-skills --agent=cursor --update

→ Updating Aristo skills for Cursor …
  • Skill content matches embedded version (v[..]); nothing to update.

ok: 0 skills updated, 4 skills already current.
```
