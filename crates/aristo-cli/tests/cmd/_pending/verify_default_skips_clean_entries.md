# `aristo verify` — default `--rerun=no` skips already-clean entries

Source: `../aretta-sdk/docs/diagrams/03-verify-execution.mmd` § `rr=no → skip ("skip clean verified/tested entries")`.

Default `aristo verify` is idempotent: a second run, with no source changes between them, is a near-zero-cost no-op. The diagram makes this explicit — the `--rerun?` decision node defaults to `no`, which routes to a `skip` step that filters out entries already in a clean `verified` or `tested` state before the per-entry mining/test pipeline runs.

This scenario is the second-half of the `--rerun` semantics asserted standalone (the force-re-verify case is in `verify_rerun_keeps_clean_entries.md`). Together they pin down "what does the index look like, and what work happens, on the second `aristo verify`?".

The first run is the standard mining pipeline (covered by `verify_test_free_full_pipeline.md`); this scenario starts from that clean state and asserts the second run is a no-op.

## First run does the work; second run is a no-op

```console
$ aristo verify

→ Running verification (free tier; local skills only) …
→ Mining assertions via aristo-mine-assertions skill ([..]) … 3 generated
[..]
ok: 3 annotations verified (method: test).
  • rebalance_postcondition   status: tested
  • dedup_invariant           status: tested
  • commit_atomicity          status: tested

$ aristo verify

→ Running verification (free tier; local skills only) …
note: 3 annotations are already in a clean verified/tested state; skipping
      (use `--rerun` to force re-verification).

ok: 0 annotations verified, 3 skipped (already clean).

```

## Editing one source file un-clean's only its own annotations; others stay skipped

```console
$ aristo stamp
ok: 3 annotations stamped, 0 ids assigned.
  • rebalance_postcondition   text unchanged, body changed — status reset to unknown
  • dedup_invariant           unchanged
  • commit_atomicity          unchanged

$ aristo verify

→ Running verification (free tier; local skills only) …
note: 2 annotations are already in a clean verified/tested state; skipping.
      • dedup_invariant
      • commit_atomicity

→ Mining assertions via aristo-mine-assertions skill ([..]) … 1 generated
  • rebalance_postcondition   → .aristo/specs/[..].spec
[..]
ok: 1 annotation verified (method: test), 2 skipped (already clean).
  • rebalance_postcondition   status: tested

```

## `--check` (CI gate) treats already-clean entries as still passing

```console
$ aristo verify --check
[..]
ok: 3 annotations verified at the strongest method available on this tier
    (3 already clean, 0 freshly verified).

```
