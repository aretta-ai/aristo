# `aristo show <id>` — full record + reverse-walked children

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "By id".

The id selector returns the annotation's full index entry (status, verify, file:line, hashes, server binding) followed by every annotation that lists it as a `parent` (the reverse-walk per F3-b).

```console
$ aristo show balance_no_duplicate_cells

balance_no_duplicate_cells (intent)
  status:    verified  (last_verified_at_commit: [..])
  verify:    "full"
  file:      core/storage/btree.rs:[..]
  site:      fn balance_non_root
  covered_region: function
  text_hash: sha256:[..]  (current — index in sync with source)
  body_hash: sha256:[..]  (current — index in sync with source)
  linked:    [..]  (server-bound)
  sig:       [..]

  Text:
    For all B-tree balance operations, no cells are duplicated: each
    cell from the input pages appears exactly once in the output pages.

  Children (annotations with parent = "balance_no_duplicate_cells"):
    cells_extracted_without_aliasing  (intent, verify=full, status=verified)
      core/storage/btree.rs:[..]  — inner cell-collection for-loop
    cumulative_counts_disjoint        (intent, verify=full, status=verified)
      core/storage/btree.rs:[..]  — cumulative-count assignment
    edit_page_writes_each_cell_once   (intent, verify=full, status=tested)
      core/storage/btree.rs:[..]  — fn edit_page
    cell_array_indices_in_bounds      (intent, verify=test, status=tested)
      core/storage/btree.rs:[..]  — fn balance_non_root  [also child of balance_no_cells_lost]
```
