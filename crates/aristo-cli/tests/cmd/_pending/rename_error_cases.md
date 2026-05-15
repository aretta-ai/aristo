# `aristo rename` — error paths

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Error cases".

Covers F1-b (reject readable → opaque), F1-c (allow opaque → readable, with note), and the duplicate-id collision case.

## Target id already in use

```console
$ aristo rename balance_no_duplicate_cells balance_op_unique_cells
? 1
error: id `balance_op_unique_cells` is already in use at src/btree.rs:[..].
       Pick a different id or delete the conflicting annotation first.
```

## Reject readable → opaque (F1-b)

```console
$ aristo rename balance_no_duplicate_cells aret_xyz123
? 1
error: id `aret_xyz123` uses the reserved `aret_` prefix (stamp-assigned only).
       Renaming a readable id to an opaque one is not supported.
       If you intended to make this annotation unaliased, delete the `id` arg
       in source and re-run `aristo stamp` — stamp will assign an opaque id.
```

## Allow opaque → readable (F1-c) with promotion note

```console
$ aristo rename aret_a1b2c3d4 post_balance_validator
ok: renamed 1 annotation
note: promoted opaque id → readable id. Future references to
      `aret_a1b2c3d4` will fail. Update any external dashboards / links.
```
