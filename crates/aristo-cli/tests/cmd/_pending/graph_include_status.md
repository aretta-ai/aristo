# `aristo graph --include-status` — color-by-status with status counts

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → `--include-status` — status-colored".

`--include-status` swaps the color axis from verify level to current B5b status. Verify level moves to a small in-node label. Use when the dominant question is "what's still unverified". Stderr surfaces a summary of counts per state.

```console
$ aristo graph --include-status --out=status.svg --format=svg

→ Reading .aristo/index.toml … ok
→ Color axis: B5b status (verified=green, tested=blue, stale=orange,
                          orphan=purple, forged=red, unknown=gray)
→ Verify level: shown as in-node label

  • verified:        [..]
  • tested:          [..]
  • stale:           [..]   ⚠
  • orphan:          [..]
  • forged:          [..]
  • unknown:         [..]

→ Shelling out to `dot -Tsvg` … ok
ok: wrote [..] nodes, [..] edges to status.svg
```
