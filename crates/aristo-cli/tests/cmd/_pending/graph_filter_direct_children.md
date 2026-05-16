# `aristo graph --filter parent=` `--depth=1` — direct children of one annotation

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Filtering — direct children only".

Shows only the immediate child annotations of a chosen root. `--depth=N` caps how far the walk goes.

```console
$ aristo graph --filter parent=aristos:balance_no_duplicate_cells --depth=1

→ Reading .aristo/index.toml … ok
→ Filtering: direct children of `aristos:balance_no_duplicate_cells`
  • [..] children
ok: rendered [..] nodes (root + [..]), [..] edges (Mermaid to stdout)

```
