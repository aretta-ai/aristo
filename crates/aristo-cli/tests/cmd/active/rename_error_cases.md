# `aristo rename` — error paths + opaque promotion

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Error cases".

Per the slice-32 scope trim (HANDOFF-SLICE-32.md, locked 2026-05-18)
and the §13 canon-and-matching design (canon-strategy.md §CS13,
locked 2026-05-19):

- Cross-namespace rejection in the original spec ("aristos: → bare;
  use `aristo unbind` instead") is replaced by a rejection of EITHER
  canon-bound namespace (`aristos:` AND `kanon:`) as source or target.
- The §13 design retires `aristo sync` entirely (the B5a-revised
  binding flow is subsumed by the canon accept path). Canon prefixes
  are applied exclusively by `aristo critique --apply-findings`;
  canon bindings are removed by `aristo canon unbind`.
- The rejection message points users at `aristo canon unbind`
  rather than at the retired `aristo sync` command.

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
       Note: `aristos:` and `kanon:` are also reserved; they may only appear
       via the canon accept path (`aristo critique --apply-findings`),
       never via `aristo rename`.
       If you intended to make this annotation unaliased, delete the `id` arg
       in source and re-run `aristo stamp` — stamp will assign an opaque id.

```

## Canon-bound namespaces (`aristos:` / `kanon:`) rejected by rename

Per §CS13 the canon-bound prefixes are reserved for the canon accept
path. To remove a canon binding, the user runs `aristo canon unbind`;
the rejection message points there. Both directions reject —
canon-bound as source AND canon-bound as target.

```console
$ aristo rename parent_id aristos:server_bound_intent
? 1
error: the `aristos:` namespace is reserved for canon-bound ids and may not
       appear as a rename source or target. `aristo rename` is for bare → bare
       or `aret_*` → bare renames only.
       To remove a canon binding, use `aristo canon unbind <aristos:<id>>`.
       Canon prefixes are applied exclusively by the canon accept path
       (`aristo critique --apply-findings`).

```

## Opaque → readable promotion (F1-c) — only success case in this scenario

```console
$ aristo rename aret_a1b2c3d4 post_validator
ok: renamed `aret_a1b2c3d4` → `post_validator` (1 source edits, 0 parent references, 0 artifact files)
note: promoted opaque id → readable id. Future references to
      `aret_a1b2c3d4` will fail. Update any external dashboards / links.

```
