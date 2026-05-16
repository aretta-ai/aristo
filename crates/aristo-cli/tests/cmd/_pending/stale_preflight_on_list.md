# Stale-index preflight on `aristo list` (J5)

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `idx -. preflight .-> r_list["aristo list"]` + `docs/mockups/11-gap-closures/cli-sessions.md` § J5.

Completes the J5 freshness-preflight scenario coverage for `aristo list` (the existing `stale_index_preflight.md` covers show / graph / status / doc but not list / review / badge). The same advisory wording fires uniformly across all reader commands; the scenario isolates the `list` invocation as the trigger.

Advisory only — exit code unchanged. Output flows: warning to stderr, normal listing to stdout, regardless of staleness.

## `aristo list` emits the stale-index warning when source is newer than the index

```console
$ aristo list
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

  aristos:balance_no_duplicate_cells       intent  verify=full   status=verified
  aristos:edit_page_writes_each_cell_once  intent  verify=full   status=stale       ⚠
  cells_extracted_without_aliasing         intent  verify=full   status=verified
[..]
[..] annotations.
```

## After `aristo stamp` the warning disappears

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned.

$ aristo list
  aristos:balance_no_duplicate_cells       intent  verify=full   status=verified
  aristos:edit_page_writes_each_cell_once  intent  verify=full   status=stale       ⚠
[..]
[..] annotations.
```

## Composes with `--filter` (J2 unified grammar) — warning still emitted, filter still applies

```console
$ aristo list --filter status=verified
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

  aristos:balance_no_duplicate_cells       intent  verify=full   status=verified
  cells_extracted_without_aliasing         intent  verify=full   status=verified
[..]
[..] matches.
```
