# `aristo graph --filter file=` — per-file scope with parent context

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Filtering — per-file".

`--filter file=<path>` includes annotations IN the file plus their immediate external parents (so the rendered graph isn't disconnected from the rest of the project).

```console
$ aristo graph --filter file=core/storage/btree.rs

→ Reading .aristo/index.toml … ok
→ Filtering: annotations in core/storage/btree.rs (+ immediate parents for context)
  • [..] in-file nodes + [..] external-parent nodes = [..] nodes
ok: rendered [..] nodes, [..] edges (Mermaid to stdout)

```
