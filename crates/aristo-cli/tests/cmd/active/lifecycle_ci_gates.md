# Workflow: CI gates — the `--check` invocations the starter workflow runs

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "5 · CI gates (on push)" + `.github/workflows/aristo.yml` written by `aristo init`.

The CI sequence the starter workflow runs on every push. Any non-zero
exit fails the build. This scenario captures the happy path (all
green); the per-command scenarios cover individual failure modes.

**Gate order (slice 34, load-bearing):**

1. `aristo stamp --check` — index in sync with source. Fail fast if
   stale, since everything downstream reads the index.
2. `aristo lint --check --strict` — cheapest static check; no LLM.
3. `aristo verify --check --strict` — heaviest read-only check;
   reports any annotations in non-clean status states. Neural
   verification has a side-effect that this gate ENQUEUES pending
   entries under `.aristo/verify-queue/pending/` for the
   `aristo-neural-verify` skill to drain — the gate itself exits
   0; the message is informational ("CI passes, but you have work
   to do locally").
4. `aristo doc --check` — purely derived from index; closes the loop.

Then two informational steps (no exit-code gating):

5. `aristo status` — emits the per-pipeline verification rate +
   tier + visible score in human-readable form.
6. `aristo badge --out=docs/badge.svg` — regenerates the SVG so the
   CI workflow can upload it as an artifact.

```console
$ aristo stamp --check
→ Walking source from [..] …
→ Found 2 annotations
→ Building index entries
→ Detecting cycles in parent graph
  new: 0, unchanged: 2, body-drifted: 0, text-changed: 0, removed: 0

ok: index is up to date (no rewrite needed).

```

```console
$ aristo lint --check --strict
ok: 0 lint findings.

```

```console
$ aristo verify --check --strict
ok: 0 annotations verified, 0 skipped (documentation only).
→ 2 entries pending neural verification — enqueued under .aristo/verify-queue/pending/.
  In Claude Code (or another agent with the aristo-neural-verify skill installed), run:
    /aristo-neural-verify
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
  By status:         neural=2

Verification rate (verified / total per pipeline):
  neural:            2 / 2 (100.0%)
  test:              0 / 0 (n/a)
  full:              0 / 0 (n/a)

Tier:
  Score:             [..]  (visible)
  Tier:              [..]

Index health:
  schema_version: 1 (current)

[INFO] For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`).

```

```console
$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..], score=[..], tier=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](https://aretta.dev/[..]/badge.svg)

```
