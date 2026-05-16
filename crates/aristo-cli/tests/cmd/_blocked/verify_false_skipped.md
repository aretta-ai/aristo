# `aristo verify` — `verify = false` annotations are skipped (documentation-only)

Source: `../aretta-sdk/docs/diagrams/03-verify-execution.mmd` § `vlvl=false → noop` ("skip (documentation only)").

`verify = false` is the explicit "this annotation is documentation only — do not run any verification on it" path. It applies on every tier: free, paid, audit-only. Running `aristo verify` on a project where every annotation is `verify = false` is a fast no-op that still updates the index status to reflect the explicit skip (so `aristo show` / `aristo list` can display "skipped (documentation only)" rather than the ambiguous "unknown"). No mining, no spec-write, no cargo-test, no signed outcome.

The contrast with `verify = "neural"` / `"test"` / `"full"` is the test below: `verify = false` exits before the per-method dispatch.

## Whole-project run where every annotation is `verify = false`

```console
$ aristo verify

→ Running verification (free tier; local skills only) …

note: 4 annotations are marked verify = false (documentation only); skipped.
      • module_intent              (src/lib.rs:[..])
      • module_layout_assume       (src/lib.rs:[..])
      • crate_root_intent          (src/lib.rs:[..])
      • design_decision_assume     (src/lib.rs:[..])

ok: 0 annotations verified, 4 skipped (documentation only).

```

## Mixed: `verify = false` is skipped, others run normally

```console
$ aristo verify

→ Running verification (free tier; local skills only) …

note: 1 annotation is marked verify = false (documentation only); skipped.
      • module_layout_assume       (src/lib.rs:[..])

→ Mining assertions via aristo-mine-assertions skill ([..]) … [..] generated
→ Compiling with aristo_verify cargo feature … ok
→ Running existing test suite with injected assertions … [..] passed

ok: [..] annotations verified, 1 skipped (documentation only).

```

## CI gate: `verify = false` does not fail `--check`

```console
$ aristo verify --check
[..]
ok: [..] annotations verified at the strongest method available on this tier.

```
