# Workflow: CI gates — the `--check` invocations the starter workflow runs

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "5 · CI gates (on push)" + `.github/workflows/aristo.yml` written by `aristo init`.

The CI sequence the starter workflow runs on every push. Any non-zero
exit fails the build. This scenario captures the happy path (all
green); the per-command scenarios cover individual failure modes.

**Gate order (load-bearing):**

1. `aristo stamp --check` — index in sync with source. Fail fast if
   stale, since everything downstream reads the index.
2. `aristo lint --check --strict` — cheapest static check; no LLM.
3. `aristo doc --check` — purely derived from index; closes the loop.

Then two informational steps (no exit-code gating):

4. `aristo status` — emits the per-pipeline verification rate +
   tier + visible score in human-readable form.
5. `aristo badge --out=docs/badge.svg` — regenerates the SVG so the
   CI workflow can upload it as an artifact.

**Note on `verify --check`:** slice 34 originally included
`aristo verify --check --strict` as the third gate, between `lint`
and `doc`. It was removed from CI on 2026-05-19 because
`aristo verify` surfaces `CliError::NotImplemented` whenever the
workspace has any `verify="test"` or `verify="full"` annotation —
both pipelines are post-MVP per `docs/deferred/verify-test-design.md`.
The command itself still works for neural-only workspaces (verified
below); CI just can't gate on it universally. Re-add the step to
both the `aristo.yml` workflow and this scenario once slice 24 (free-
tier test path) ships.

```console
$ aristo stamp --check
→ Walking source from [..] …
→ Found 2 annotations
→ Checking for parent-link cycles
  status (from .aristo/proofs/): fresh: 0, stale: 0, refuted: 0, unverified: 2

ok: index is up to date (no rewrite needed).

```

```console
$ aristo lint --check --strict
ok: 0 lint findings.

```

```console
$ aristo verify --check --strict
ok: 0 annotations verified, 0 skipped (documentation-only).
→ 2 entries pending neural verification — enqueued under .aristo/verify-queue/pending/.
  In Claude Code (or another agent with the aristo-verify skill installed), run:
    /aristo-verify
  to produce verdicts for each pending entry. The skill writes .aristo/proofs/<id>.proof
  files; run `aristo verify --apply-verdicts` to validate and apply them to the index.

```

```console
$ aristo doc --check
→ Reading .aristo/index.toml … ok
→ Computing expected per-annotation markdown …
→ Comparing against .aristo/doc/ on disk …

ok: doc artifacts are in sync with the index.

```

```console
$ aristo status

Aristo SDK v[..]
  Default verify:    "neural"

Annotations:
  Total:             2
  By kind:           intent=2   assume=0
  By verify level:   neural=2   test=0   full=0   true=0   false=0
  By status:         unknown=2

Verification rate (verified / total per pipeline):
  neural:            0 / 2 (0.0%)
  test:              0 / 0 (n/a)
  full:              0 / 0 (n/a)

Tier:
  Score:             [..]  (visible)
  Tier:              [..]

Index health:
  schema_version: 1 (current)

Canon binding:
  Auth:              no token (free-tier mode)
  Last fetched:      never
  Catalog version:   —
  Pending:           0
  Accepted (bound):  0
  Rejected:          0

For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`).

```

```console
$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](docs/badge.svg)

```
