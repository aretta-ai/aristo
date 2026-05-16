# `aristo index` — standalone source walk + index regenerate

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `w_index["aristo index --all"] -- regenerate --> idx` + `docs/TOOLS.md` row "aristo index".

`aristo index` is the canonical indexer (per D2): walk the source tree, parse `.rs` files via `syn`, find annotations, compute hashes, detect cycles in the parent graph, write `.aristo/index.toml` atomically. Incremental by default (per-file mtime cache); `--all` forces a full rewalk that ignores the cache. Idempotent.

Distinction from `aristo stamp`: `aristo index` is the pure walk-and-write step (writes hashes, ids, parent links). `aristo stamp` runs `aristo index` and additionally classifies B5b binding state and assigns ids — `stamp` is the all-in-one developer-facing command, `index` is the lower-level building block useful for CI auditors and third-party tooling that wants the index without committing to the full stamp pipeline.

`aristo index` is not in the freshness-preflight list — it *is* the refresh path, so it never emits "index may be stale" warnings (per `docs/diagrams/02-state-map.mmd`).

## Default incremental run (only changed files re-walked)

```console
$ aristo index
→ Walking source from . … 47 files scanned, 3 changed since last run
→ Parsing 3 files via syn … ok
→ Computing hashes … ok
→ Detecting cycles in parent graph … ok (no cycles)
→ Writing .aristo/index.toml … ok ([..] entries, [..] bytes)

ok: index regenerated (incremental: 3 files re-walked, 44 from cache).
```

## `--all` forces a full rewalk

```console
$ aristo index --all
→ Walking source from . … 47 files scanned (--all: cache ignored)
→ Parsing 47 files via syn … ok
→ Computing hashes … ok
→ Detecting cycles in parent graph … ok (no cycles)
→ Writing .aristo/index.toml … ok ([..] entries, [..] bytes)

ok: index regenerated (full: 47 files re-walked).
```

## Cycle detection runs in `aristo index` (matches `aristo stamp`)

```console
$ aristo index
? 2

error: cycle detected in parent graph
       a → b → c → a

Break the cycle by removing one of these parent links:
  • a (src/lib.rs:[..])    has parent = "c"
  • b (src/lib.rs:[..])    has parent = "a"
  • c (src/lib.rs:[..])    has parent = "b"

No files modified. Fix the cycle and re-run `aristo index`.
```

## Re-running with no source change is a no-op

```console
$ aristo index
→ Walking source from . … 47 files scanned, 0 changed since last run
ok: index up to date (no rewrite).
```
