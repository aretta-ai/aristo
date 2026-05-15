# `aristo rename --dry-run` — preview without writes

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Dry-run preview".

Demonstrates that `--dry-run` plans the full coordinated rename (source edits, spec file move, index updates) and reports server-binding side effects, but writes nothing.

```console
$ aristo rename --dry-run balance_no_duplicate_cells balance_op_unique_cells

Plan: rename `balance_no_duplicate_cells` → `balance_op_unique_cells`

Source edits:
  src/btree.rs:[..]    id = "balance_no_duplicate_cells"   →   id = "balance_op_unique_cells"
  src/btree.rs:[..]    parent = "balance_no_duplicate_cells"   →   parent = "balance_op_unique_cells"
  src/btree.rs:[..]    parent = ["balance_no_duplicate_cells", "balance_no_cells_lost"]
                       →   parent = ["balance_op_unique_cells", "balance_no_cells_lost"]

Spec file:
  .aristo/specs/balance_no_duplicate_cells.spec → .aristo/specs/balance_op_unique_cells.spec
  Internal annotation_id field will update accordingly.

Index updates:
  [balance_no_duplicate_cells]  →  [balance_op_unique_cells]
  edit_page_writes_each_cell_once.parent: "balance_no_duplicate_cells" → "balance_op_unique_cells"
  cell_array_indices_in_bounds.parent: [.., "balance_no_duplicate_cells", ..] → [.., "balance_op_unique_cells", ..]

⚠️  This annotation is server-bound (aristos: namespace per B5a revised).
   Renaming WITHIN the aristos: namespace keeps the prefix and invalidates
   the index sig; status reverts to "unknown" pending re-bind.
   Run `aristo sync --rebind aristos:balance_op_unique_cells` afterward.
   (To remove the binding entirely, use `aristo unbind aristos:<id>` instead.)

(no changes written — dry-run)
```
