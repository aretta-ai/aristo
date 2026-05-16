# `aristo lint --fix` — auto-fix safe rules (J6)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J6 — Configurable lint pre-commit mode" (`"fix"` mode).

`--fix` applies auto-fixable rules (whitespace, casing, explicit `auto_fix = true` rules). Never applies semantic rewrites. The pre-commit hook's `"fix"` mode wraps this and re-stages modified files, but the staging is git-side; the trycmd scenario captures only the `aristo lint --fix` invocation itself.

```console
$ aristo lint --fix
fixed: 2 whitespace issues across 1 file

```
