# `aristo graph --out=<file>` — write to disk instead of stdout

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Write to file".

`--out=<path>` redirects rendered output to a file; format inferred from `--format` (default Mermaid). Atomic via temp-file + rename — concurrent readers never see a partial write.

```console
$ aristo graph --out=docs/annotations.mmd
? 0
ok: wrote 3 nodes, 1 edges to docs/annotations.mmd

```
