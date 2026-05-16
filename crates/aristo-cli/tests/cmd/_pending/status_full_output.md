# `aristo status` — tier, quota, B5b binding counts, footer hint (J7)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J7 — `aristo status` enumeration hint".

`aristo status` prints tier (with Founding Member / Design Partner badge if applicable), quota, total annotations broken down by verify level, and B5b server-binding counts (verified / stale / pending-deepen / orphan / forged). J7 added the trailing footer line `ℹ For per-annotation diagnostics …` so users seeing a non-zero count know which command surfaces the per-annotation detail.

```console
$ aristo status

Aristo SDK v[..]
  Tier:              Pro (Founding Member, [..]/50 seats)
  Default verify:    "full"
  Quota:             [..] / [..] credits remaining this month

Annotations:
  Total:             [..]
  By verify level:   neural=[..]   test=[..]   full=[..]

Server-bound (aristos:):
  Verified:          [..]  ✓
  Stale:             [..]  ⚠
  Pending-deepen:    [..]  ⚠
  Orphan:             0
  Forged:             0

Index health:
  schema_version: 1 (current)
  Bundled key registry: scheme v1, [..] active keys, [..] revoked

ℹ For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`).

```
