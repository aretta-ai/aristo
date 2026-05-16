# Uniform stale-index preflight across read commands (J5)

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J5 — Uniform index-freshness preflight".

J5 established a shared internal preflight that runs at the start of every CLI command which READS `.aristo/*` artifacts. Compares per-file source mtimes against the index's mtime cache; if any source file is newer than its indexed entry, emits one stderr warning recommending `aristo stamp` and continues with the (possibly stale) index. Advisory only — exit code unchanged.

The same wording appears across `aristo show`, `aristo graph`, `aristo verify` (incl. `--audit-only`), `aristo doc`, `aristo status`, `aristo badge`, `aristo list`, `aristo review`. Refresh commands (`aristo stamp`, `aristo index`) do not emit it; they are the refresh path.

## On `aristo show`

```console
$ aristo show fn balance_non_root
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

aristos:balance_no_duplicate_cells (intent)
  status:    verified  (last_verified_at_commit: [..])
[..]

```

## On `aristo graph`

```console
$ aristo graph --format=svg --out=graph.svg
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.
→ Rendering DOT graph …
ok: wrote [..] nodes, [..] edges to graph.svg

```

## On `aristo status`

```console
$ aristo status
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

Aristo SDK v[..]
  Tier:              [..]
[..]

```

## On `aristo doc --check` — does not affect exit code separately from doc-sync result

```console
$ aristo doc --check
? 1
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.
error: 1 doc artifact out of sync with the index.

```

## After `aristo stamp` the warning disappears

```console
$ aristo stamp
ok: [..] annotations stamped, [..] ids assigned.

$ aristo show fn balance_non_root
aristos:balance_no_duplicate_cells (intent)
  status:    verified  …

```
