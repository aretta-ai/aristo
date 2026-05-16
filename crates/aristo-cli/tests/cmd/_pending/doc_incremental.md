# `aristo doc` — incremental regeneration

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → Incremental run (only changed annotations touched)".

Subsequent runs only rewrite files whose rendered content actually changed. Cuts CI doc-regeneration time and keeps git diffs minimal.

```console
$ aristo doc

→ Reading .aristo/index.toml … ok ([..] annotations)
→ Generating per-annotation markdown …
  • Updated: .aristo/doc/aristos__balance_no_duplicate_cells.md  (text changed)
  • Unchanged: [..] files

ok: doc artifacts updated. ([..] written, [..] unchanged)

```
