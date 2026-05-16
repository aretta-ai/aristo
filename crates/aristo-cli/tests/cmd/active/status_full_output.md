# `aristo status` — project-level summary

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J7 — `aristo status` enumeration hint".

Phase-1 subset of the J7 output: reads `aristo.toml` for the default verify level, the index for the annotation breakdown (by kind / verify level / status), and reports the index schema version. Phase 2 fields (tier, quota, B5b binding counts, bundled key registry) wait for the server-side commands.

The sandbox pre-populates `src/lib.rs` with 2 intents + 1 assume (see `.in/` fixture).

## Phase-1 baseline + stamp + status

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo stamp
→ Walking source from [..] …
→ Found 3 annotations
→ Building index entries
→ Detecting cycles in parent graph
  new: 3, unchanged: 0, body-drifted: 0, text-changed: 0, removed: 0

ok: stamped 3 annotations into .aristo/index.toml

$ aristo status

Aristo SDK v[..]
  Default verify:    (per-tier default)

Annotations:
  Total:             3
  By kind:           intent=2   assume=1
  By verify level:   neural=1   test=1   full=0   true=0   false=0
  By status:         unknown=3

Index health:
  schema_version: 1 (current)

[INFO] For per-annotation diagnostics, run `aristo stamp` (or `aristo list --filter status=<state>`).

```
