# Tiered badge default — `aristo badge --metric=tier`

Source: `docs/decisions/badge-tier-scheme.md` D7 (visible-score formula),
D8 (tier cutoffs), D11 (visual palette + locked logo).

Slice 31.5 makes `tier` the default headline metric for `aristo badge`,
replacing slice 31's `aristos-count`. The slice-31 surface stays
accessible via `--metric=count`; the verification-rate variant from the
visibility-artifacts mockup is exposed as `--metric=rate`. The
`--metric` flag changes which value lands in the SVG body — the
progress lines always report `aristos-count`, `verification-rate`,
`score`, and `tier` so the diagnostic surface is uniform regardless of
which headline metric the user picked.

## Default invocation places the tier in the SVG value half

```console
$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](docs/badge.svg)

```

## `--metric=count` preserves the slice-31 total-count surface

```console
$ aristo badge --metric=count --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](docs/badge.svg)

```

## `--metric=rate` exposes verification-rate as the headline value

```console
$ aristo badge --metric=rate --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](docs/badge.svg)

```

## `--metric=tier` is identical to the default

```console
$ aristo badge --metric=tier --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](docs/badge.svg)

```

## Unknown `--metric` value is rejected at parse time

```console
$ aristo badge --metric=quality --out=docs/badge.svg
? 2
error: unknown --metric `quality`; expected `count`, `rate`, or `tier`

```
