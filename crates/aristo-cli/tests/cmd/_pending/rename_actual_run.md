# `aristo rename` — actual coordinated rename

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F1 → "Actual rename".

Without `--dry-run`, the rename writes all source/spec/index edits atomically and emits a server-binding warning when the renamed annotation was bound.

```console
$ aristo rename balance_no_duplicate_cells balance_op_unique_cells

ok: renamed 1 annotation, updated 2 parent references, 1 spec file
warning: server-binding sig invalidated by id change
         Run `aristo sync --rebind aristos:balance_op_unique_cells` to re-bind
         (per B5a revised: source aristos: prefix preserved; index sig refreshes on re-bind)
```
