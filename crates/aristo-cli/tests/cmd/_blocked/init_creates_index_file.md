# `aristo init` creates an empty `.aristo/index.toml`

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `w_init -- creates --> idx`.

Settles the open question flagged in `docs/WORKFLOW-COVERAGE.md` §1.2: does `aristo init` create the index file, or does the first `aristo stamp` create it? Per the diagram, **`aristo init` creates all four state files** (`conf`, `idx`, `spcs`, `dcs`) — it is the only `init` writer in the diagram. So `init` writes `.aristo/index.toml` with the `[__meta__]` header and zero annotation entries; subsequent `aristo stamp` adds entries as it discovers annotations in source.

This rule is what lets every read command (`aristo show`, `aristo list`, `aristo status`, …) safely assume `.aristo/index.toml` exists in any initialized project — they error with "run `aristo init`" if absent, never "run `aristo stamp`".

## `aristo init` creates the index file with a meta-only body

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: installed pre-commit hook (.git/hooks/pre-commit)
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ cat .aristo/index.toml
[__meta__]
schema_version = "[..]"
generated_by   = "aristo init [..]"
generated_at   = "[..]"
```

## Read commands work on a freshly-initialized project (zero annotations is a valid state)

```console
$ aristo list
0 annotations.

$ aristo status
Aristo SDK v[..]
  Tier:              [..]
  Annotations:       0
  Verified:          0 (n/a)
  Index:             .aristo/index.toml ([..])
[..]
```

## `aristo init` in an already-initialized project is a no-op (idempotent)

```console
$ aristo init
note: aristo.toml already exists — leaving as-is.
note: .aristo/index.toml already exists — leaving as-is.
note: .aristo/specs/ already exists.
note: .aristo/doc/ already exists.
note: pre-commit hook already installed.
note: .github/workflows/aristo.yml already exists.
ok: nothing to do.
```
