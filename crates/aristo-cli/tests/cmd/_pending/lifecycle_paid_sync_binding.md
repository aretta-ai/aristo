# Workflow: paid user binds an annotation to the server (sync + verified_outcome)

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "3 · Server binding (paid; when ready to upstream)".

Walks the paid-tier binding flow: `aristo auth login` → `aristo sync` (server matches the annotation against its property template, applies the `aristos:` namespace prefix in source, populates `linked` + `verified_outcome` in the index) → `aristo show` confirms the certificate.

The interactive bits of `auth login` (browser flow, etc.) are stubbed in `[..]` here; the activated form will use a fixture credential or `--token <env-var>` for non-interactive testing.

```console
$ aristo auth login
[..]
ok: authenticated as [..] (Pro tier).

$ aristo sync
→ Authenticating with aretta.dev … ok (Pro tier)
→ Uploading 1 unbound annotation for template matching …
→ Server matched 1 annotation against property template `bt_balance_invariant_v1`
→ Applying `aristos:` namespace prefix in source (atomic) … src/btree.rs:[..]
→ Writing linked + verified_outcome to index entry …

ok: 1 annotation bound.
  • aristos:rebalance_invariant   linked: arta_[..]   status: verified

$ aristo show aristos:rebalance_invariant
aristos:rebalance_invariant  (intent)
  status:    verified  ✓
  verify:    "full"  (method used: refinement-proof)
[..]
  Verification certificate:
    verified_outcome: v1:[..]
    signed_at:        [..]
    commit_hash:      [..]  (HEAD)
    signature:        Ed25519, valid against bundled public key (scheme v[..])
[..]
```
