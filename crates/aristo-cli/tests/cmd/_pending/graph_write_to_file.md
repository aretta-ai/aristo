# `aristo graph --out=<file>` — write to disk instead of stdout

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Write to file".

`--out=<path>` redirects rendered output to a file; format inferred from `--format` (default Mermaid).

```console
$ aristo graph --out=docs/annotations.mmd

→ Reading .aristo/index.toml … ok
→ Rendering Mermaid graph …
ok: wrote [..] nodes, [..] edges to docs/annotations.mmd

```
