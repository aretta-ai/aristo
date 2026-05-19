# `aristo rename` — actual coordinated rename

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Actual rename".

Per the slice-32 scope trim (HANDOFF-SLICE-32.md, locked 2026-05-18),
this scenario uses bare ids end-to-end. Without `--dry-run`, the rename
writes the source byte-substitution + index rewrite + (when present)
per-id artifact moves atomically. Apply order is source files first,
artifact moves next, index LAST — so a partial failure leaves the
source ahead of the index and `aristo stamp` will detect drift.

The success line reports counts in parens: `(N source edits, K parent
references, P artifact files)`. The fixture has no `.critique` / `.proof`
artifacts so `P = 0`.

Verifying side effects: the `.out` half of this fixture pins the
post-rename state byte-for-byte (`src/lib.rs` has the new id at both
the `id = ` site and the `parent = ` site; the index has the renamed
self-key plus the rewritten child parent link).

```console
$ aristo rename balance_no_duplicate_cells balance_op_unique_cells
ok: renamed `balance_no_duplicate_cells` → `balance_op_unique_cells` (2 source edits, 1 parent references, 0 artifact files)

```
