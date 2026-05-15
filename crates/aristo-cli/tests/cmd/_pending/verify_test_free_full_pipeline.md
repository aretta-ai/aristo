# `aristo verify` — `verify = "test"` on free tier (full mining pipeline)

Source: `../aretta-sdk/docs/diagrams/03-verify-execution.mmd` § `t_tier=Free → t_free → t_spec → t_feat → t_ct → out_status`.

The free-tier `verify = "test"` path is the full local mining pipeline:

1. `t_free` — invoke `aristo-mine-assertions` skill via the host coding agent
2. `t_spec` — write the mined assertion to `.aristo/specs/<id>.spec`
3. `t_feat` — toggle the `aristo_verify` cargo feature
4. `t_ct` — run `cargo test` (existing test suite) with assertions injected via the macro
5. `out_status` — record the per-annotation verdict in the index entry's `status`

This scenario is the **clean baseline** for the path. The J4 free-tier `"full"`-downgrade scenario (`verify_free_tier_downgrade.md`) shows the same pipeline reached via a downgrade *note*; this scenario shows it as the developer's primary observable on annotations they explicitly tagged `verify = "test"`.

The spec file is the load-bearing intermediate artifact: present after the run, used by `cargo test --features aristo_verify` to inject assertions, and re-used on subsequent runs (per the diagram's `spcs ... include via fv` edge in `02-state-map.mmd`).

## Default run

```console
$ aristo verify

→ Running verification (free tier; local skills only) …

→ Mining assertions via aristo-mine-assertions skill ([..]) … 3 generated
  • rebalance_postcondition   → .aristo/specs/[..].spec
  • dedup_invariant           → .aristo/specs/[..].spec
  • commit_atomicity          → .aristo/specs/[..].spec

→ Compiling with aristo_verify cargo feature … ok
→ Running existing test suite with injected assertions … 47 passed, 1 failed
  • rebalance_postcondition   status: tested  (assertion fired in 12 tests)
  • dedup_invariant           status: tested  (assertion fired in 8 tests)
  • commit_atomicity          status: tested  (assertion did not fire — no covering test)

ok: 3 annotations verified (method: test).
```

## Spec files exist after a successful run

```console
$ ls .aristo/specs/
[..].spec
[..].spec
[..].spec

$ aristo show rebalance_postcondition
rebalance_postcondition (intent)
  status:    tested
  method:    test
  binding:   local
  spec:      .aristo/specs/[..].spec
[..]
```

## Re-running with no source change is a no-op (idempotent)

```console
$ aristo verify

→ Running verification (free tier; local skills only) …
note: 3 annotations are already in a clean verified/tested state; skipping
      (use `--rerun` to force re-verification).

ok: 0 annotations verified, 3 skipped (already clean).
```
