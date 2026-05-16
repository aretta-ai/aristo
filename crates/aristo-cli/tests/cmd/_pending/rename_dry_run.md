# `aristo rename --dry-run` — preview without writes

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Dry-run preview".

Demonstrates that `--dry-run` plans the full coordinated rename (source edits, spec file move, index updates) and reports server-binding side effects per B5a-revised + B5b, but writes nothing.

```console
$ aristo rename --dry-run aristos:balance_no_duplicate_cells aristos:balance_op_unique_cells

Plan: rename `aristos:balance_no_duplicate_cells` → `aristos:balance_op_unique_cells`

Source edits:
  src/btree.rs:[..]    id = "aristos:balance_no_duplicate_cells"   →   id = "aristos:balance_op_unique_cells"
  src/btree.rs:[..]    parent = "aristos:balance_no_duplicate_cells"   →   parent = "aristos:balance_op_unique_cells"
  src/btree.rs:[..]    parent = ["aristos:balance_no_duplicate_cells", "balance_no_cells_lost"]
                       →   parent = ["aristos:balance_op_unique_cells", "balance_no_cells_lost"]

Spec file:
  .aristo/specs/aristos__balance_no_duplicate_cells.spec → .aristo/specs/aristos__balance_op_unique_cells.spec
  Internal annotation_id field will update accordingly.

Index updates:
  ["aristos:balance_no_duplicate_cells"]  →  ["aristos:balance_op_unique_cells"]
  edit_page_writes_each_cell_once.parent: "aristos:balance_no_duplicate_cells" → "aristos:balance_op_unique_cells"
  cell_array_indices_in_bounds.parent: [.., "aristos:balance_no_duplicate_cells", ..] → [.., "aristos:balance_op_unique_cells", ..]

⚠️  This annotation is server-bound (aristos: namespace per B5a revised + B5b).
   Renaming WITHIN the aristos: namespace keeps the prefix and invalidates
   the index `verified_outcome` (signed payload includes `annotation_id`);
   status reverts to "unknown" pending re-bind.
   Run `aristo sync --rebind aristos:balance_op_unique_cells` afterward.
   Cross-namespace renames (e.g., `aristos:foo` → `bar`) are rejected;
   use `aristo unbind aristos:<id>` instead to remove the binding entirely.

(no changes written — dry-run)

```
