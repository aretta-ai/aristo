# `aristo graph --filter status=` — J3 status-filtered visualization

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J3 — `aristo graph --filter status=...`".

J3 added `status=<state>` to the unified filter grammar's surfacing on `aristo graph`. Useful for review-meeting questions like "show me what's still unverified": filter to one status, optionally combine with `--include-status` (existing) to color by status, render to SVG for sharing.

## SVG render of one status, with neighbor context

```console
$ aristo graph --filter status=unknown --format=svg --out=review.svg

→ Reading .aristo/index.toml … ok
→ Filtering: [..] annotations with status=unknown (+ immediate neighbors for context)
→ Shelling out to `dot -Tsvg` … ok
ok: wrote [..] nodes, [..] edges to review.svg

```

## Compose with `--include-status` to color by status

```console
$ aristo graph --filter status=stale --include-status
[..]

```
