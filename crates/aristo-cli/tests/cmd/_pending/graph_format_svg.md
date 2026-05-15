# `aristo graph --format=svg` — shells out to `dot -Tsvg`

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → SVG format (shells out to `dot`)".

For SVG, Aristo emits DOT internally and shells out to `dot -Tsvg`. Requires Graphviz on PATH; the missing-`dot` error path is captured separately in `graph_format_svg_no_dot.md`.

```console
$ aristo graph --format=svg --out=annotations.svg

→ Reading .aristo/index.toml … ok
→ Rendering DOT graph in memory …
→ Shelling out to `dot -Tsvg` … ok (graphviz [..])
ok: wrote [..] nodes, [..] edges to annotations.svg
```
