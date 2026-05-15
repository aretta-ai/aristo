# `aristo show` — error paths and advisory output

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "Did-you-mean (typo)" + "Stale-index warning" + "Missing case: empty result".

Covers F3-c (strict id match with did-you-mean), the J5 stale-index preflight surfacing on `aristo show`, and the empty-result hint with next-step suggestions.

## Did-you-mean on typo'd id (F3-c)

```console
$ aristo show balance_no_dupes
? 1
error: no annotation with id `balance_no_dupes`.

Did you mean:
  • aristos:balance_no_duplicate_cells  (core/storage/btree.rs:[..])
```

## Stale-index advisory (J5 preflight)

```console
$ aristo show aristos:balance_no_duplicate_cells

⚠️  .aristo/index.toml is older than [..] .rs files. Output below may be stale.
   Re-run `aristo index` to refresh.

aristos:balance_no_duplicate_cells (intent)
[..]
```

## Empty result with next-step hints

```console
$ aristo show fn does_not_exist
No items matching `fn does_not_exist` found in indexed source files.

Try:
  • `aristo show --list-functions` to see all indexed functions
  • `aristo index` to refresh if you've added new sources
```
