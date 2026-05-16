# `aristo rename` — error paths

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Error cases".

Covers the duplicate-id collision case, cross-namespace rename rejection (per TOOLS.md rename rule), F1-b (reject readable → opaque, widened to all reserved namespaces `aret_*` / `aristos:*`), and F1-c (allow opaque → readable, with promotion note).

## Target id already in use

```console
$ aristo rename aristos:balance_no_duplicate_cells aristos:balance_op_unique_cells
? 1
error: id `aristos:balance_op_unique_cells` is already in use at src/btree.rs:[..].
       Pick a different id or delete the conflicting annotation first.

```

## Cross-namespace rename rejected (use `aristo unbind` instead)

```console
$ aristo rename aristos:balance_no_duplicate_cells balance_op_unique_cells
? 1
error: cross-namespace rename rejected
       (`aristos:` → bare id is not a rename — it's an unbind).
       Use `aristo unbind aristos:balance_no_duplicate_cells` to drop the
       server binding (preserving the local id), or pick a target inside
       the same namespace (e.g., `aristos:balance_op_unique_cells`).

```

## Reject rename to reserved prefix (F1-b)

```console
$ aristo rename balance_no_duplicate_cells aret_xyz123
? 1
error: id `aret_xyz123` uses the reserved `aret_` prefix (stamp-assigned only).
       Renaming a readable id to an opaque one is not supported.
       Note: `aristos:` is also reserved; it may only appear via
       `aristo sync` binding, never via `aristo rename`.
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
