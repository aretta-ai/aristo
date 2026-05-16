# `aristo verify --rerun` — force re-verification of already-clean entries

Source: `../aretta-sdk/docs/diagrams/03-verify-execution.mmd` § `rr=yes → keep ("keep all matching entries")`.

`--rerun` is the orthogonal force flag (per J2). With `--rerun`, the `keep` branch is taken instead of `skip`, so already-clean `verified` / `tested` entries do participate in the per-entry pipeline. The asserted contrast: without `--rerun`, the second `aristo verify` call after a clean run is a no-op (`verify_default_skips_clean_entries.md`); with `--rerun`, every matching entry runs again.

`--rerun` composes with `--filter`: `--filter` selects which entries are eligible; `--rerun` forces eligible-and-clean entries to run anyway. A bare `--rerun` (no filter) is the "after a server key rotation, force everything" form — captured separately in `verify_filter_rerun.md`; this scenario isolates the *semantics* of "would have been skipped without --rerun".

## Setup: first run brings everything to a clean state

```console
$ aristo verify

→ Running verification (free tier; local skills only) …
→ Mining assertions via aristo-mine-assertions skill ([..]) … 3 generated
[..]
ok: 3 annotations verified (method: test).

```

## Without `--rerun`: clean entries are skipped (the default-skip case)

```console
$ aristo verify

→ Running verification (free tier; local skills only) …
note: 3 annotations are already in a clean verified/tested state; skipping
      (use `--rerun` to force re-verification).

ok: 0 annotations verified, 3 skipped (already clean).

```

## With `--rerun`: same-input clean entries run again, end up clean again

```console
$ aristo verify --rerun

→ Running verification (free tier; local skills only) …
note: --rerun set — re-verifying 3 entries already in a clean state.

→ Mining assertions via aristo-mine-assertions skill ([..]) … 3 generated
  • rebalance_postcondition   → .aristo/specs/[..].spec  (rewritten)
  • dedup_invariant           → .aristo/specs/[..].spec  (rewritten)
  • commit_atomicity          → .aristo/specs/[..].spec  (rewritten)

→ Compiling with aristo_verify cargo feature … ok
→ Running existing test suite with injected assertions … 47 passed
  • rebalance_postcondition   status: tested
  • dedup_invariant           status: tested
  • commit_atomicity          status: tested

ok: 3 annotations verified (method: test).

```

## `--rerun --filter status=verified` re-runs only the previously-clean subset

```console
$ aristo verify --rerun --filter status=verified

→ Running verification (free tier; local skills only) …
note: --rerun set — re-verifying 3 entries already in a clean state.

→ Mining assertions via aristo-mine-assertions skill ([..]) … 3 generated
[..]
ok: 3 annotations verified (method: test).

```
