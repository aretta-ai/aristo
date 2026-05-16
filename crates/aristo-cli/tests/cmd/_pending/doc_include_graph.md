# `aristo doc --include-graph` — convenience: doc + summary + graph SVG

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → `--include-graph` convenience".

Composite flag: invokes `aristo doc --summary` plus `aristo graph --format=svg --out=.aristo/doc/_graph.svg`, then embeds the graph in `_summary.md` so crate-root rustdoc renders the annotation graph above the per-annotation list. Requires Graphviz `dot` on PATH (same prerequisite as `aristo graph --format=svg`).

```console
$ aristo doc --include-graph

→ Reading .aristo/index.toml … ok
→ Generating per-annotation markdown … ([..] files written)
→ Generating crate summary → .aristo/doc/_summary.md
→ Generating graph SVG → .aristo/doc/_graph.svg
  • Shelling out to `dot` … ok (graphviz [..])
  • [..] nodes, [..] edges, [..] root component

ok: doc artifacts + graph updated.

The summary now embeds the graph at the top.

```
