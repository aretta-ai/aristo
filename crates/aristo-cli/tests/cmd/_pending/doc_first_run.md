# `aristo doc` — first run, full generation

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I1 → First run".

Walks `.aristo/index.toml`, writes one markdown file per annotation to `.aristo/doc/<id-safe>.md` (where `<id-safe>` replaces `:` with `__` for filesystem safety). On a clean target dir, every file is written. Footer hints the user toward enabling the `aristo_doc` cargo feature and embedding `_summary.md`.

```console
$ aristo doc

→ Reading .aristo/index.toml … ok ([..] annotations: [..] intent, [..] assume)
→ Generating per-annotation markdown to .aristo/doc/ …
  • Wrote: .aristo/doc/aristos__balance_no_duplicate_cells.md
  • Wrote: .aristo/doc/aristos__edit_page_writes_each_cell_once.md
  • Wrote: .aristo/doc/cells_extracted_without_aliasing.md
  • Wrote: .aristo/doc/cell_array_indices_in_bounds.md
  • Wrote: .aristo/doc/storage_write_atomicity.md
[..]
  ([..] files written, 0 unchanged)

ok: doc artifacts updated.

Next steps:
  • Enable the `aristo_doc` cargo feature in your Cargo.toml:
        [features]
        default = ["aristo_doc"]
  • Or run `cargo doc --features aristo_doc` to render docs with annotations.
  • Optional: add `#[doc = include_str!(".aristo/doc/_summary.md")]`
    above your crate's `//!` doc to render the project-level summary.
```
