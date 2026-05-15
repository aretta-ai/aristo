# `aristo graph --format=svg` — friendly error when `dot` is missing

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → SVG without `dot` installed".

Exits non-zero with platform-specific install hints AND alternatives that don't require `dot` (`--format=dot` and `--format=mermaid` both work without Graphviz).

```console
$ aristo graph --format=svg --out=annotations.svg
? 2
error: SVG output requires Graphviz `dot`, which was not found on PATH.

Install:
  • macOS:    brew install graphviz
  • Debian:   apt install graphviz
  • Windows:  https://graphviz.org/download/

Alternatives:
  • aristo graph --format=dot > annotations.dot
    (then render with any Graphviz tool)
  • aristo graph --format=mermaid > annotations.mmd
    (renders in any markdown viewer that supports Mermaid)
```
