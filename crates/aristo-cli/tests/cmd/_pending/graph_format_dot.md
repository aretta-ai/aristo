# `aristo graph --format=dot` — Graphviz DOT output

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → DOT format".

Universal interop format. Pipes into any Graphviz tool for SVG/PNG/PDF rendering. No `dot` install required from Aristo's side — we just emit text.

```console
$ aristo graph --format=dot --out=annotations.dot

→ Reading .aristo/index.toml … ok
→ Rendering DOT graph …
ok: wrote [..] nodes, [..] edges to annotations.dot

To render:
  dot -Tsvg annotations.dot -o annotations.svg
  dot -Tpng annotations.dot -o annotations.png

```
