# `aristo list` — flat inventory with stale-index advisory

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.a — `aristo list`".

Default text output: one annotation per line with id, kind, verify level, status; sorted alphabetically by id; footer summary by kind and binding state. The J5 stale-index preflight emits its standard advisory header when the index is older than source.

```console
$ aristo list
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

  aristos:balance_no_duplicate_cells       intent  verify=full   status=verified
  aristos:edit_page_writes_each_cell_once  intent  verify=full   status=stale       ⚠
  aristos:page_type_discriminants_…        intent  verify=full   status=verified
  cell_array_indices_in_bounds             intent  verify=test   status=tested
  cells_extracted_without_aliasing         intent  verify=full   status=verified
  storage_write_atomicity                  assume  —             status=unknown
  aret_a1b2c3d4 (post_balance_validation)  intent  verify=false  status=unknown
[..]

47 annotations  (33 intent · 14 assume · 20 server-bound)
```
