# `aristo graph --format=dot` — Graphviz DOT output

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → DOT format".

Universal interop format. Pipes into any Graphviz tool for SVG / PNG / PDF rendering. Aristo does NOT shell out for this format — we just emit text. When written to a file via `--out`, prints the standard Graphviz render invocations as a hint so the user knows what to run next.

```console
$ aristo graph --format=dot --out=annotations.dot
? 0
ok: wrote 3 nodes, 1 edges to annotations.dot

To render:
  dot -Tsvg annotations.dot -o annotations.dot.svg
  dot -Tpng annotations.dot -o annotations.dot.png

```
