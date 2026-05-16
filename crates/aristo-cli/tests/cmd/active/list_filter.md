# `aristo list --filter` — unified filter grammar (J2)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.a → Filtering uses the unified grammar".

Single-filter and multi-filter (AND-semantics) selection; uses the J2 unified filter grammar shared with `aristo verify`, `aristo graph`, `aristo review`. Forms: `id=<id>`, `file=<path>`, `parent=<id>`, `status=<state>`. Multiple `--filter` flags AND together.

## Single filter — status

```console
$ aristo list --filter status=stale
  aristos:edit_page_writes_each_cell_once  intent  verify=full   status=stale       ⚠
1 match.
```

## Multiple filters AND together

```console
$ aristo list --filter file=core/storage/btree.rs --filter status=verified
  aristos:balance_no_duplicate_cells       intent  verify=full   status=verified
  cells_extracted_without_aliasing         intent  verify=full   status=verified
[..]
8 matches.
```
