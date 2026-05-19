# Stale-index preflight on `aristo badge` (J5)

> **Deferred from `active/` 2026-05-19** — trycmd runs commands in-place
> against the committed `.in` fixture, which arrives in CI with uniform
> mtimes (checkout writes everything at the same time). The first block
> of this scenario asserts the "source newer than index" stale-warning
> path, which can't fire in a clean CI checkout. Locally it passed
> because mtime drift from prior runs made the fixture's source files
> newer than the index. Additionally, slice 31.5 extended the badge
> metric line with `score` and `tier` fields; the existing
> `aristos-count=[..], verification-rate=[..]` single-line wildcard
> doesn't span the new fields cleanly. The badge command itself stays
> covered by `badge_tier_default.md`; the stale-preflight wiring is
> covered by per-command unit tests of the preflight call. To
> re-promote: add a setup step that explicitly `touch`es source files
> newer than `.aristo/index.toml`, and extend the wildcard line to
> `aristos-count=[..], verification-rate=[..], score=[..], tier=[..]`.

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
