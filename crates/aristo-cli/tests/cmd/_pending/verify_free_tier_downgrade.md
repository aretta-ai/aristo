# `aristo verify` — free-tier `verify="full"` graceful downgrade (J4)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J4 — Free-tier `verify=\"full\"` graceful downgrade".

J4 revised G1: free-tier `aristo verify` no longer errors on `verify = "full"` annotations. Instead it silently runs them under `"test"` (the strongest method available locally) and reports the downgrade as a one-line note. The source `verify = "full"` is preserved unchanged so the day the user upgrades, those annotations re-run at full strength with no source edit.

CI (`--check`) does not fail just because the user is on free; the future `--require=<method>` flag (per G3) is what gates "actually require full to count".

## Default run with downgrade note

```console
$ aristo verify

→ Running verification (free tier; local skills only) …

note: 3 annotations marked verify="full" were downgraded to "test" for this run
      (free tier; "full" requires the paid HQ verification engine — see `aristo status`)
      • rebalance_postcondition         (src/ring.rs:[..])
      • dedup_invariant                 (src/ring.rs:[..])
      • commit_atomicity                (src/store.rs:[..])

→ Mining assertions via aristo-mine-assertions skill ([..]) … [..] generated
→ Compiling with aristo_verify cargo feature … ok
→ Running existing test suite with injected assertions … [..] passed, [..] failed
  • rebalance_postcondition  status: tested  (assertion fired)
  • dedup_invariant          status: tested  (assertion fired)
  • commit_atomicity         status: tested  (assertion did not fire — no covering test)

ok: [..] annotations verified ([..] downgraded full→test).

```

## CI gate doesn't fail on tier alone

```console
$ aristo verify --check
[..]
ok: [..] annotations verified at the strongest method available on this tier.

```
