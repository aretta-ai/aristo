# Workflow: edit source → `aristo stamp` surfaces drift on the affected annotation

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "2 · Daily authoring loop" — `L → l1 → l3` chain (developer edits source, then re-stamps).

This is the load-bearing daily-loop chain in the diagram: the developer edits a covered region, re-runs `aristo stamp`, and the index entry's status flips to reflect the now-stale verification (because the body hash drifted from the value baked into the previous `verified_outcome` / `tested` state). The previously-verified status moves to `unknown`; subsequent `aristo verify` re-mines the assertion and brings it back to clean.

The `stamp_cycle_diagnostics.md` scenario covers the standalone cycle-detection diagnostics; this scenario covers the steady-state daily-edit chain where stamp is the early-warning system.

## Setup: clean state — annotation is verified, body hash recorded

```console
$ aristo show rebalance_invariant
rebalance_invariant (intent)
  status:    tested
  method:    test
  binding:   local
  body_hash: sha256:[..]
  text_hash: sha256:[..]
[..]
```

## Edit source body, then `aristo stamp` flips status to `unknown` with a "body changed" note

```console
$ aristo stamp
ok: 1 annotation stamped, 0 ids assigned.
  • rebalance_invariant       text unchanged, body changed — status reset to unknown

$ aristo show rebalance_invariant
rebalance_invariant (intent)
  status:    unknown
  method:    test
  binding:   local
  body_hash: sha256:[..]
  text_hash: sha256:[..]

note: body hash changed since last verification.
      Previously: tested (body_hash sha256:[..])
      Run `aristo verify` to re-verify, or `aristo verify --filter id=rebalance_invariant`.
```

## `aristo verify` brings it back to clean

```console
$ aristo verify

→ Running verification (free tier; local skills only) …
note: 0 annotations are already in a clean verified/tested state.
→ Mining assertions via aristo-mine-assertions skill ([..]) … 1 generated
[..]
ok: 1 annotation verified (method: test).
  • rebalance_invariant       status: tested
```

## Editing only annotation TEXT (not body) flips to `text-drift` instead — re-review path

```console
$ aristo stamp
ok: 1 annotation stamped, 0 ids assigned.
  • rebalance_invariant       text changed, body unchanged — status held; review-cache invalidated
```
