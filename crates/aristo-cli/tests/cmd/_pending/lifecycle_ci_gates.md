# Workflow: CI gates — the five `--check` invocations the starter workflow runs

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "5 · CI gates (on push)" + `.github/workflows/aristo.yml` written by `aristo init`.

The CI sequence the starter workflow runs on every push. Any non-zero exit fails the build. This scenario captures the happy path (all green); the per-command scenarios (`stamp_cycle_diagnostics.md`, `verify_audit_only_check.md`, `doc_check_fails.md`, `lint_check_fail.md`, etc.) cover individual failure modes.

```console
$ aristo stamp --check
ok: index in sync with source. ([..] annotations)

$ aristo verify --check --strict
ok: [..] annotations verified.
  No stale, orphan, forged, or pending-deepen findings.

$ aristo doc --check
ok: [..] doc artifacts in sync with the index.

$ aristo lint --check --strict
ok: [..] annotations linted, no findings (info / warn / error).
```
