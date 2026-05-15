# `aristo graph --filter id=` — subtree rooted at one annotation

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Filtering — subtree under one root".

`--filter id=<id>` renders only the subtree under the chosen root (transitive descendants). Useful for focused review of a single property's downstream chain.

```console
$ aristo graph --filter id=aristos:balance_no_duplicate_cells --out=balance.mmd

→ Reading .aristo/index.toml … ok
→ Filtering: subtree rooted at `aristos:balance_no_duplicate_cells`
  • Root + [..] descendants ([..] levels deep)
ok: wrote [..] nodes, [..] edges to balance.mmd
```
