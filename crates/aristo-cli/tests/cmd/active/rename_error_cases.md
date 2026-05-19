# `aristo rename` — error paths + opaque promotion

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Error cases".

Per the slice-32 scope trim (HANDOFF-SLICE-32.md, locked 2026-05-18):

- The cross-namespace rejection case in the original spec ("aristos: →
  bare; use `aristo unbind` instead") is replaced by the broader
  "Phase 2 deferred" message — `aristos:` ids are rejected in EITHER
  direction (source or target) until `aristo sync` ships.
- The `aristo unbind` hint is dropped (that surface is Phase 2 alongside
  sync; pointing users at an unimplemented command would be a worse
  diagnostic than the deferred-Phase-2 explanation).

Four cases run sequentially against the same `.in` workspace. The first
three are errors (no state changes); the fourth (F1-c opaque-to-readable
promotion) is the only one that modifies state. The `.out` reflects the
post-promotion state.

## Target id already in use

```console
$ aristo rename parent_id taken_target
? 1
error: id `taken_target` is already in use at src/lib.rs:fn taken_fn (line 13).
       Pick a different id or delete the conflicting annotation first.

```

## Reject rename to reserved `aret_` prefix (F1-b)

```console
$ aristo rename parent_id aret_xyz1234
? 1
error: id `aret_xyz1234` uses the reserved `aret_` prefix (stamp-assigned only).
       Renaming a readable id to an opaque one is not supported.
       Note: `aristos:` is also reserved; it may only appear via
       `aristo sync` binding, never via `aristo rename`.
       If you intended to make this annotation unaliased, delete the `id` arg
       in source and re-run `aristo stamp` — stamp will assign an opaque id.

```

## `aristos:` namespace deferred to Phase 2 sync (scope trim)

Both directions reject with the same Phase 2 message — when the target
is `aristos:` the user wants a rebind (Phase 2); when the source is
`aristos:` the user wants an unbind (Phase 2). Either way, `aristo
sync` is the future home.

```console
$ aristo rename aristos:server_bound_intent bar
? 1
error: the `aristos:` namespace is reserved for server-bound ids
       (Phase 2). `aristo rename` is local-only in this release; the
       rebind / unbind surface ships with `aristo sync`.
       For bare → bare or `aret_*` → bare renames, use this command.
       For aristos: ids, wait for Phase 2 sync.

```

## Opaque → readable promotion (F1-c) — only success case in this scenario

```console
$ aristo rename aret_a1b2c3d4 post_validator
ok: renamed `aret_a1b2c3d4` → `post_validator` (1 source edits, 0 parent references, 0 artifact files)
note: promoted opaque id → readable id. Future references to
      `aret_a1b2c3d4` will fail. Update any external dashboards / links.

```
