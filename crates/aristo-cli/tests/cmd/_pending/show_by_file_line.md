# `aristo show <file>:<line>` — locate annotations covering a line

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "By file:line".

The file:line selector returns annotations whose covered region includes the line — including β-form annotations attached upstream of the queried line.

```console
$ aristo show core/storage/btree.rs:3058

Annotation covering this line:
  cells_extracted_without_aliasing  (intent, verify=full, status=verified)
    @ core/storage/btree.rs:[..] (β-form, covers the for-loop at line 3058)
    parent: balance_no_duplicate_cells

  Text:
    Each cell pushed here is a distinct memory reference:
    _cell_get_raw_region_faster returns non-overlapping (start, len)
    regions, so successive pushes never alias.
```
