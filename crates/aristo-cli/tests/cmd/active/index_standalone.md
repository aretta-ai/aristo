# `aristo index` — standalone source walk + index regenerate

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `w_index["aristo index --all"] -- regenerate --> idx` + `docs/TOOLS.md` row "aristo index".

`aristo index` is the canonical indexer (per D2): walk the source tree, parse `.rs` files via `syn`, find annotations, compute hashes, detect cycles in the parent graph, write `.aristo/index.toml` atomically.

Distinction from `aristo stamp` (slice 17+): `aristo index` is the pure walk-and-write step (writes hashes, ids, parent links). `aristo stamp` runs `aristo index` and additionally classifies B5b binding state and offers id-promotion via the rename flow — `stamp` is the all-in-one developer-facing command, `index` is the lower-level building block useful for CI auditors and third-party tooling.

`aristo index` is not in the freshness-preflight list — it *is* the refresh path, so it never emits "index may be stale" warnings (per `docs/diagrams/02-state-map.mmd`).

Slice 16 ships the full-walk path; the per-file mtime cache that the original mockup-12 sketch describes (`incremental: 3 files re-walked, 44 from cache`) is a slice-17+ optimization. `--all` is accepted as a no-op flag in this slice so users / CI scripts that already pass it don't break when the cache lands.

## Default run on a freshly-initialized project (zero annotations)

```console
$ aristo init
...

$ aristo index
→ Walking source from [..] …
→ Found 0 annotations
→ Building index entries
→ Detecting cycles in parent graph
→ Writing .aristo/index.toml … ok (0 entries, [..] bytes)

ok: index regenerated (0 annotations).

```

## `--all` is accepted as a no-op (mtime cache lands in slice 17+)

The flag is wired through clap and accepted; behavior is identical to the no-flag form for slice 16.

```console
$ aristo init
...

$ aristo index --all
→ Walking source from [..] …
→ Found 0 annotations
...
ok: index regenerated (0 annotations).

```

The "walk requires a workspace" error path is exercised by the imperative test in `crates/aristo-cli/tests/index_command.rs::errors_outside_a_workspace` — it can't share the trycmd sandbox with the success cases above (init in the previous block creates the workspace).
