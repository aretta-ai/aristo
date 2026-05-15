# `aristo verify --filter` and `--rerun` — J2 selector overhaul

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J2 — Verify `--filter` and `--rerun` rework".

J2 promoted `--filter` from future to defined and simplified `--rerun` to a bare flag (no positional, no `--stale` shortcut). The unified grammar (`id=` / `file=` / `parent=` / `status=`) is shared with `aristo list`, `aristo graph`, `aristo review`. `--filter` selects which annotations participate; `--rerun` is the orthogonal force-flag for re-running entries already in a clean verified state.

The pre-J2 forms — `aristo verify --rerun aristos:edit_page_writes_each_cell_once` and `aristo verify --rerun --stale` — are removed and not tested.

## Filter by id

```console
$ aristo verify --filter id=aristos:edit_page_writes_each_cell_once
[..]
```

## Filter by status

```console
$ aristo verify --filter status=stale
[..]
```

## Filter by file path

```console
$ aristo verify --filter file=core/storage/btree.rs
[..]
```

## `--rerun` composes with `--filter`

```console
$ aristo verify --rerun --filter status=verified
[..]
```

## Bare `--rerun` re-verifies everything (rare; for after a server key rotation)

```console
$ aristo verify --rerun
[..]
```

## Composes with `--check` and `--strict`

```console
$ aristo verify --filter file=core/storage/btree.rs --check
[..]

$ aristo verify --filter status=stale --check --strict
[..]
```
