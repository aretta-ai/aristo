# `aristo doc --check` — CI gate against out-of-sync artifacts

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → `--check` (CI gate)".

Recomputes the expected per-annotation markdown from the index and diffs against `.aristo/doc/` on disk. Non-zero exit on any drift. Catches the developer who edited an annotation in source, ran `aristo stamp`, but forgot to re-run `aristo doc` before committing.

```console
$ aristo doc --check
? 1
→ Reading .aristo/index.toml … ok
→ Computing expected per-annotation markdown …
→ Comparing against .aristo/doc/ on disk …
  • Out of sync: .aristo/doc/aristos__balance_no_duplicate_cells.md
    (text in index does not match rendered markdown)

error: 1 doc artifact out of sync with the index.
       Run `aristo doc` locally and commit the result.

```
