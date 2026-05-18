# `aristo doc --summary` — project-level summary markdown

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → `--summary`".

Emits `.aristo/doc/_summary.md` with project-level counts (kind, verify level, server-bound state). Users include it via a one-line `#![doc = include_str!("../.aristo/doc/_summary.md")]` in their crate's `//!` doc. NOT auto-edited into source — Aristo never source-rewrites.

```console
$ aristo doc --summary

→ Reading .aristo/index.toml … ok
→ Writing .aristo/doc/_summary.md
  • [..] annotations ([..] intent · [..] assume)
  • [..] server-bound (aristos: namespace)
  • Verify levels: false=[..], neural=[..], test=[..], full=[..]

ok: crate-root summary written.

To render in `cargo doc`, add to your lib.rs / main.rs:
    //! ...your existing crate doc...
    #![doc = include_str!("../.aristo/doc/_summary.md")]

```
