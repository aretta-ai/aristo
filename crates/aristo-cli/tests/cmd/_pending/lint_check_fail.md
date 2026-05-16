# `aristo lint --check` — fail-fast in pre-commit hook (J6)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J6 — Configurable lint pre-commit mode" (default `"check"` mode).

J6 made `[lint] pre_commit` a string enum (`"off"` / `"check"` / `"fix"`). `"check"` (default) runs `aristo lint --check` in the hook — fail-fast, never silently modifies staged content. This scenario captures the direct CLI invocation; the git-commit-hook wrapper is fixture-dependent and lives in an imperative `assert_cmd` test.

```console
$ aristo lint --check
? 1
error: 1 lint finding (error severity)
  • cache_eviction_pre (src/cache.rs:[..]): empty_text (annotation text is empty)

```
