# B5b state: `verified` — issuance, offline re-validation, full record

Source: `../aretta-sdk/docs/mockups/09-signature-scheme/cli-sessions.md` § "State 1: verified — the happy path".

The happy path: `aristo verify` runs HQ verification on the paid tier, receives an Ed25519-signed `verified_outcome` from the server, writes it into the index. Subsequent `aristo stamp` runs offline-revalidate every signature each invocation. `aristo show` exposes the certificate block.

## Initial sync + verify (server issues `verified_outcome`)

```console
$ aristo verify --filter id=aristos:balance_no_duplicate_cells

→ Authenticating with aretta.dev … ok (Pro tier, [..] credits remaining)
→ Uploading annotation + covered region (core/storage/btree.rs::balance_non_root) … [..] KB
→ Running HQ full-verification … [..]s
  method:     refinement proof against btree model
  status:     verified
→ Receiving signed outcome … ok
  verified_outcome: v1:[..]
  commit_hash:      [..] (HEAD)
  signed_at:        [..]

ok: 1 annotation verified.
  • aristos:balance_no_duplicate_cells  (status: verified, method: refinement-proof)
    Index entry updated; signature recorded.

```

## Subsequent `aristo stamp` re-validates each signature offline

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned
  • aristos:balance_no_duplicate_cells   verified-outcome: valid ✓
  • aristos:edit_page_writes_each_cell_once   verified-outcome: valid ✓
  • aristos:page_type_discriminants_are_format_stable   verified-outcome: valid ✓
[..]

```

## `aristo show` exposes the certificate block

```console
$ aristo show aristos:balance_no_duplicate_cells

aristos:balance_no_duplicate_cells  (intent)
  status:    verified  ✓
  verify:    "full"  (method used: refinement-proof)
  file:      core/storage/btree.rs:[..]
  site:      fn balance_non_root
  text_hash: sha256:[..]  (current — index in sync with source)
  body_hash: sha256:[..]  (current — index in sync with source)
  linked:    arta_[..]  (server-bound)

  Verification certificate:
    verified_outcome: v1:[..]
    signed_at:        [..]
    commit_hash:      [..]  (HEAD)
    signature:        Ed25519, valid against bundled public key (scheme v[..])

  Text:
    For all B-tree balance operations, no cells are duplicated…

```
