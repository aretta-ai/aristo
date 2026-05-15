# Workflow: bootstrap a fresh project + first verify (free tier)

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "1 · Setup" + "2 · Daily authoring loop".

Walks the canonical first-time path: `aristo init` → install skills for the user's agent → developer adds an annotation → pre-commit-hook-equivalent `aristo stamp` runs → `aristo verify` runs free-tier locally.

This scenario is a multi-step trycmd that establishes the integration contract between the four commands. Each step's standalone behavior is captured in the per-command scenarios; this file asserts they compose end-to-end.

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/{specs,doc}/
ok: installed pre-commit hook (.git/hooks/pre-commit)
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo install-skills --agent=cursor
[..]
ok: 4 skills installed for cursor.
[..]

$ aristo stamp
ok: 1 annotation stamped, 1 id assigned
  • aret_[..] (rebalance_invariant)   intent  verify=test  status=unknown

$ aristo verify
[..]
→ Mining assertions via aristo-mine-assertions skill ([..]) … [..] generated
→ Compiling with aristo_verify cargo feature … ok
→ Running existing test suite with injected assertions … [..] passed
ok: 1 annotation verified.
  • rebalance_invariant   status: tested
```
