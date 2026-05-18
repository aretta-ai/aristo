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
...
ok: stamped [..] annotations into [..]

$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](https://aretta.dev/[..]/badge.svg)

```

## Stdout target (no `--out`) emits any advisory to stderr; SVG to stdout is not corrupted

The block above re-stamped the index, so the freshness warning will NOT
fire here — but the SVG must still appear on stdout uncorrupted. The
leading `...` absorbs whatever advisory lines are or aren't emitted; the
explicit `<svg [..]` … `</svg>` framing locks the SVG shape. (When this
scenario runs against a stale index — without a preceding stamp — the
warning lands on stderr; trycmd's per-file sandbox model makes the
warning here state-dependent across blocks, but the stdout SVG-framing
guarantee does NOT depend on freshness.)

```console
$ aristo badge
...
<svg [..]
...
</svg>

```
