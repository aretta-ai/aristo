# Stale-index preflight on `aristo review` (J5)

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `idx -. preflight .-> r_review["aristo review"]` + `docs/mockups/11-gap-closures/cli-sessions.md` § J5.

Completes the J5 freshness-preflight coverage for `aristo review`. Same advisory wording, same advisory-only contract (exit code unchanged), regardless of whether review itself finds anything. The warning fires before review begins so the user knows the review is being run against a possibly stale annotation list.

Note: `aristo review` reads annotation text from source files via the index (the index points to file:line locations); a stale index can mean review runs on text that no longer matches the body. The advisory is what tells the user that.

## `aristo review` emits the stale-index warning when source is newer than the index

```console
$ aristo review
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

running review… (using aristo-review-skill via [..], model=off-the-shelf)

ok: reviewed [..] annotations in [..]
  cached:  [..] (unchanged since last review)
  fresh:   [..] (re-reviewed this run)

findings: [..] ([..] strong-suggest, [..] suggest, [..] info)
[..]
```

## After `aristo stamp` the warning disappears

```console
$ aristo stamp
ok: [..] annotations stamped, 0 ids assigned.

$ aristo review
running review… (using aristo-review-skill via [..], model=off-the-shelf)

ok: reviewed [..] annotations in [..]
[..]
```

## Composes with `--filter` — warning still emitted

```console
$ aristo review --filter id=balance_no_duplicate_cells
warning: .aristo/index.toml may be stale relative to source ([..] files newer than indexed).
         Run `aristo stamp` to refresh.

running review… (1 target)

balance_no_duplicate_cells (core/storage/btree.rs:[..])
[..]
```
