# `aristo list` — flat inventory with summary footer

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.a — `aristo list`".

Default text output: one annotation per line with id, kind, verify level, status; sorted alphabetically by id; footer summary by kind. The J5 stale-index preflight advisory header is deferred to slice 19 (which ships the shared preflight).

The sandbox pre-populates `src/lib.rs` with two intents + one assume (see `.in/` fixture).

## Phase-1 baseline + three-annotation walk → sorted listing

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo list

0 annotations  (0 intent / 0 assume)

$ aristo stamp
→ Walking source from [..] …
→ Found 3 annotations
→ Building index entries
→ Detecting cycles in parent graph
  new: 3, unchanged: 0, body-drifted: 0, text-changed: 0, removed: 0

ok: stamped 3 annotations into .aristo/index.toml

$ aristo list
  alpha                 intent  verify=test    status=unknown
  bravo                 intent  verify=full    status=unknown
  charlie               assume  verify=-       status=unknown

3 annotations  (2 intent / 1 assume)

```
