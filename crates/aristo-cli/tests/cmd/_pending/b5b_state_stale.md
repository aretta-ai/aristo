# B5b state: `stale` — code drifted, signature still cryptographically valid

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "State 2: stale — code drifted; signature still real".

A developer edits an annotated function's body. On next `aristo stamp`, the body's token-stream hash differs from the one in `verified_outcome`. The signature itself still verifies — it just describes a past state. Status reverts from `verified` → `unknown`; user prompted to `--rerun` (per J2 unified filter grammar).

## `aristo stamp` surfaces the drift

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned
warning: 1 stale verified outcome
  • aristos:edit_page_writes_each_cell_once   (core/storage/btree.rs:[..])
    body_hash changed: prior [..], current [..]
    verified_outcome is cryptographically valid but describes the prior
    body state. Status reverted: verified → unknown.

    Re-verify with:    aristo verify --rerun --filter id=aristos:edit_page_writes_each_cell_once
    Or rebind binding: aristo sync --rebind aristos:edit_page_writes_each_cell_once

```

## `aristo show` reflects the stale state

```console
$ aristo show aristos:edit_page_writes_each_cell_once

aristos:edit_page_writes_each_cell_once  (intent)
  status:    unknown  ⚠  (was: verified — stale relative to body)
  verify:    "full"
  file:      core/storage/btree.rs:[..]
  site:      fn edit_page

  ⚠  Stale verification:
     stale_reason:    body_hash differs from signed outcome
     prior body_hash: sha256:[..]  (in signed outcome)
     current body_hash: sha256:[..]  (computed this run)
     verified_outcome is cryptographically valid; only the code drifted.

     To resolve:
       aristo verify --rerun --filter id=aristos:edit_page_writes_each_cell_once
       (or `aristo sync --rebind` if you don't want to re-run verification)

```

## `aristo verify --check` (CI gate) fails on stale

```console
$ aristo verify --check
? 1
error: 1 stale verified outcome (CI mode requires resolution)
  • aristos:edit_page_writes_each_cell_once
    Run `aristo verify --rerun --filter id=aristos:edit_page_writes_each_cell_once`
    locally, commit the updated index, and re-push.

```
