# `aristo rename --dry-run` — preview without writes

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Dry-run preview".

Per the slice-32 scope trim (HANDOFF-SLICE-32.md, locked 2026-05-18)
and §CS13 (canon-strategy.md, locked 2026-05-19), canon-bound
prefixes (`aristos:` and `kanon:`) are reserved for the canon accept
path and rejected by rename; this scenario uses bare ids end-to-end.
Demonstrates that `--dry-run` plans the full coordinated rename
(source edits across single-id + single-parent + array-parent forms,
plus index updates) and writes nothing.

The fixture contains 3 annotations in a single file: a parent intent,
a child with `parent = "..."`, and a third with `parent = [.., ..]`.

```console
$ aristo rename balance_no_duplicate_cells balance_op_unique_cells --dry-run

Plan: rename `balance_no_duplicate_cells` → `balance_op_unique_cells`

Source edits:
  src/lib.rs:4    id = "balance_no_duplicate_cells"   →   id = "balance_op_unique_cells"
  src/lib.rs:11    parent = "balance_no_duplicate_cells"   →   parent = "balance_op_unique_cells"
  src/lib.rs:19    parent = ["balance_no_duplicate_cells", "edit_page_writes_each_cell_once"]
                       →   parent = ["balance_op_unique_cells", "edit_page_writes_each_cell_once"]

Index updates:
  ["balance_no_duplicate_cells"]  →  ["balance_op_unique_cells"]
  cell_array_indices_in_bounds.parent: ["balance_no_duplicate_cells", "edit_page_writes_each_cell_once"] → ["balance_op_unique_cells", "edit_page_writes_each_cell_once"]
  edit_page_writes_each_cell_once.parent: "balance_no_duplicate_cells" → "balance_op_unique_cells"

(no changes written — dry-run)

```
