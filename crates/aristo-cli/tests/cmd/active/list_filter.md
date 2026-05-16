# `aristo list --filter` — unified filter grammar (J2)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J1.a → Filtering uses the unified grammar".

Single-filter and multi-filter (AND-semantics) selection; uses the J2 unified filter grammar shared with `aristo verify` (slice 22), `aristo graph` (slice 29), `aristo review` (slice 27). Forms: `id=<id>`, `file=<path>`, `parent=<id>`, `status=<state>`. Multiple `--filter` flags AND together.

The sandbox pre-populates `src/lib.rs` with two intents (see `.in/` fixture).

## Single filter — id

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo stamp
→ Walking source from [..] …
→ Found 2 annotations
→ Building index entries
→ Detecting cycles in parent graph
  new: 2, unchanged: 0, body-drifted: 0, text-changed: 0, removed: 0

ok: stamped 2 annotations into .aristo/index.toml

$ aristo list --filter id=alpha
  alpha                 intent  verify=test    status=unknown

1 match.  (2 total in index)

```

## Filter on an unknown key is rejected (helpful error)

```console
$ aristo list --filter kind=intent
? 2
error: unknown filter key `kind`; expected one of: id, file, parent, status

```
