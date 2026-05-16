# B5b: `pending-deepen` — shallow-clone soft warning

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "Shallow-clone soft warning".

CI environments often shallow-clone (`git fetch --depth=1`). If the outcome's `commit_hash` falls outside the shallow window, `git merge-base --is-ancestor` can't decide ancestry. SDK surfaces a soft warning (status `verified-pending-deepen`) rather than failing hard — distinguishable from genuine `orphan` or `verified` so CI doesn't false-positive on legitimate setups.

`aristo verify --audit-only --check --strict` (per J1) treats `verified-pending-deepen` as failure for downstream consumers who want a clear yes/no.

```console
$ aristo stamp

ok: [..] annotations stamped, 0 ids assigned
warning: 3 annotations cannot confirm commit ancestry (shallow clone)
  • aristos:balance_no_duplicate_cells
  • aristos:edit_page_writes_each_cell_once
  • aristos:page_type_discriminants_are_format_stable

  Their verified_outcome signatures are valid; however, the signed
  commit_hash falls outside this checkout's shallow window so we cannot
  confirm it belongs to this repository's history.

  To confirm:        git fetch --unshallow
  To gate strictly:  aristo verify --check --strict   (treats unknown as failure)
  To allow:          (default) — status marked "verified-pending-deepen"

  No changes written.

```
