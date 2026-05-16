# Stale-index preflight on `aristo badge` (J5)

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `idx -. preflight .-> r_badge["aristo badge"]` + `docs/mockups/11-gap-closures/cli-sessions.md` § J5.

Completes the J5 freshness-preflight coverage for `aristo badge`. Same advisory wording, same advisory-only contract (exit code unchanged). Specifically important for `aristo badge` because the badge metrics (`aristos-count`, `verification-rate`) come from the index — running `badge` against a stale index gives a stale public number on the README.

## `aristo badge` emits the stale-index warning when source is newer than the index

```console
$ aristo badge --out=docs/badge.svg
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](https://aretta.dev/[..]/badge.svg)

```

## After `aristo stamp` the warning disappears

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned.

$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](https://aretta.dev/[..]/badge.svg)

```

## Stdout target (no `--out`) emits the warning to stderr; SVG to stdout is not corrupted

```console
$ aristo badge
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.
<svg [..]
[..]
</svg>

```
