# Workflow: paid user binds an annotation to the server (sync + verified_outcome)

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "3 · Server binding (paid; when ready to upstream)".

Walks the paid-tier binding flow: `aristo auth login` → `aristo sync` (server matches the annotation against its property template, applies the `aristos:` namespace prefix in source, populates `linked` + `verified_outcome` in the index) → `aristo show` confirms the certificate.

The interactive bits of `auth login` (browser flow, etc.) are stubbed in `[..]` here; the activated form will use a fixture credential for non-interactive testing. Server side, the canon calls go to the org's repo-prefixed routes (`/<repo>/api/canon/...`); until the conductor serves them, the CLI is exercised against the mock client.

```console
$ aristo auth login --server https://acme.aretta.ai
[..]
ok: authenticated as [..] at https://acme.aretta.ai

$ aristo sync
→ Authenticating against https://acme.aretta.ai … ok
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
