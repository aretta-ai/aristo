# `aristo show fn <name>` — function selector + multi-match disambiguation

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "By function name" + "Multi-match disambiguation".

Covers F3-d: a single-match returns attached AND inside-body (β-form) annotations; a multi-match lists all sites with file:line for disambiguation.

## Single match — attached + β-form annotations both surfaced

```console
$ aristo show fn balance_non_root

Found 1 site matching `fn balance_non_root` in core/storage/btree.rs:[..].

Attached to fn:
  aristos:balance_no_duplicate_cells (intent, verify=full, status=verified)
    "For all B-tree balance operations, no cells are duplicated…"
  cell_array_indices_in_bounds (intent, verify=test, status=tested)
    "All accesses to cell_array are bounds-checked…"

Inside fn body (β-form annotations):
  cells_extracted_without_aliasing (intent, verify=full, status=verified)
    @ btree.rs:[..] — before for-loop
    "Each cell pushed here is a distinct memory reference…"
  cumulative_counts_disjoint (intent, verify=full, status=verified)
    @ btree.rs:[..] — before assignment
    "After this assignment, cell_count_per_page_cumulative[i] equals…"
```

## Multi-match disambiguation (F3-d)

```console
$ aristo show fn seek

Found 3 sites matching `fn seek`. Disambiguate by id, file:line, or item path:

  cursor_trait_seek_postcondition  (intent, verify=full)
    core/storage/btree.rs:[..]   — fn seek (trait CursorTrait method declaration)

  btree_cursor_seek_impl_safety  (intent, verify=test, status=tested)
    core/storage/btree.rs:[..]  — fn seek (impl BTreeCursor)

  index_cursor_seek_impl  (intent, verify=neural, status=neural)
    core/index/cursor.rs:[..]    — fn seek (impl IndexCursor)

To show one, run: `aristo show <id>` or `aristo show <file>:<line>`
```
