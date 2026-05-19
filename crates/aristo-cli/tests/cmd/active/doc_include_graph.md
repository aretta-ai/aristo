# `aristo doc --include-graph` — convenience: summary + embedded graph

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → `--include-graph` convenience".

Composite flag: implies `--summary` and additionally appends the rendered annotation graph (Mermaid `flowchart TD` block) to `_summary.md` under a `## Annotation graph` heading. Re-runs are idempotent — the append happens after the summary is re-written, so the final file has exactly one summary section + one graph section regardless of prior state. No external dependency (Mermaid renders inline in any markdown viewer that supports it, including GitHub READMEs and Mermaid-aware rustdoc themes).

```console
$ aristo doc --include-graph
? 0

→ Reading .aristo/index.toml … ok
→ Writing .aristo/doc/_summary.md
  • 3 annotations (2 intent · 1 assume)
  • 0 server-bound (aristos: namespace)
  • Verify levels: false=0, neural=0, test=0, full=2

ok: crate-root summary written.

To render in `cargo doc`, add to your lib.rs / main.rs:
    //! ...your existing crate doc...
    #![doc = include_str!("../.aristo/doc/_summary.md")]
→ Appending annotation graph to .aristo/doc/_summary.md
  • 3 nodes, 1 edges (Mermaid, embedded inline)

```
