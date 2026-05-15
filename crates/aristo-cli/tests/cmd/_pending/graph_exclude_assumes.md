# `aristo graph --exclude-assumes` — focus on intent graph

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → `--exclude-assumes` — focus on intent graph".

Hides `assume` nodes from the rendered graph. Useful when the question is "what's the verified-property structure" and the OS/library assumptions are noise.

```console
$ aristo graph --exclude-assumes

→ Reading .aristo/index.toml … ok
→ Filtering: skipping [..] assume nodes
ok: rendered [..] nodes, [..] edges (Mermaid to stdout)
```
