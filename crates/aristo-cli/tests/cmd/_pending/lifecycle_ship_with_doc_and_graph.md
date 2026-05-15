# Workflow: pre-push ship — doc + graph + badge artifacts

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "4 · Ship — before push".

Generates the artifacts a developer commits alongside source so a fresh `git clone` renders correct rustdoc + graph + README badge without running anything Aristo-side. Composite of `aristo doc --include-graph` (writes `.aristo/doc/*.md` + `_graph.svg` + `_summary.md`) and `aristo badge` (SVG for the README).

```console
$ aristo doc --include-graph
→ Reading .aristo/index.toml … ok
→ Generating per-annotation markdown … ([..] files written)
→ Generating crate summary → .aristo/doc/_summary.md
→ Generating graph SVG → .aristo/doc/_graph.svg
  • Shelling out to `dot` … ok (graphviz [..])
[..]
ok: doc artifacts + graph updated.
[..]

$ aristo badge --out=docs/badge.svg
→ Reading .aristo/index.toml … ok
→ Computing metrics: aristos-count=[..], verification-rate=[..]
→ Writing docs/badge.svg ([..] style)
ok: badge written. Embed in README:

  ![aristo verified](https://aretta.dev/[..]/badge.svg)
```
