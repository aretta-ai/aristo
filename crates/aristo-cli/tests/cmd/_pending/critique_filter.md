# `aristo critique --filter` — agentic review with J2 unified filter grammar

Source: `../aretta-sdk/docs/diagrams/01-lifecycle.mmd` § "3 · Inspect & debug" — `I → i5` ("aristo critique --filter (free tier; local skill)") + `docs/mockups/07-lint-critique-skills/examples.md` § "aristo critique".

This scenario is the only `aristo critique` coverage in the test suite (per the audit in `docs/WORKFLOW-COVERAGE.md` §1.1). `aristo critique` is the agentic, tier-aware deeper-critique command: free tier invokes the `aristo-critique` via the host coding agent (Claude Code / Cursor / etc.); paid tier uses the HQ critique agent. Output is a list of findings categorized by `[rephrasing]` / `[parent-shape]` / `[vocabulary]` / etc., severity-tagged (`strong-suggest` / `suggest` / `info`). Read-only — never modifies source.

`--filter` uses the J2 unified grammar (`id=`, `file=`, `parent=`, `status=`) shared with `aristo list` / `verify` / `graph`. Range form (`<file>:<start>-<end>`) is supported per the mockup 07 examples. `aristo critique` also auto-skips targets that have outstanding `aristo lint` findings — the diagnostic in the output points the user at `aristo lint --fix` first.

## Whole-project run (free tier, local skill)

```console
$ aristo critique
running critique… (using aristo-critique via [..], model=off-the-shelf)

ok: critiqued [..] annotations in [..]
  cached:  [..] (unchanged since last critique)
  fresh:   [..] (re-critiqued this run)

findings: [..] ([..] strong-suggest, [..] suggest, [..] info)

──────────────────────────────────────────────────────────────────────────────
balance_no_duplicate_cells (core/storage/btree.rs:[..])
  ✦ [rephrasing] strong-suggest
    Current opens with double-negation ("no cells are duplicated"). Lead
    with the positive property for clarity.
[..]

Index updated: [..] entries written `last_critiqued_at_text_hash` + finding count.

```

## Filter by id

```console
$ aristo critique --filter id=balance_no_duplicate_cells
running critique… (1 target)

balance_no_duplicate_cells (core/storage/btree.rs:[..])
  ✦ [rephrasing] strong-suggest
[..]

```

## Filter by file

```console
$ aristo critique --filter file=core/storage/btree.rs
running critique… ([..] targets)
[..]

```

## Filter by line range (`<file>:<start>-<end>`)

```console
$ aristo critique --filter "core/storage/btree.rs:6900-6950"
running critique… (1 target in line range)

edit_page_writes_each_cell_once (core/storage/btree.rs:6911)
  ✦ [vocabulary] info
[..]

```

## Multiple filters AND together (J2 shared grammar)

```console
$ aristo critique --filter file=core/storage/btree.rs --filter status=verified
running critique… ([..] targets)
[..]

```

## Targets with outstanding lint findings are skipped with a pointer to `aristo lint --fix`

```console
$ aristo critique --filter id=cell_array_borrows_from_pages
running critique… (1 target)

cell_array_borrows_from_pages (core/storage/btree.rs:[..])
  ⚠ aristo lint flagged this annotation: [text_too_long].
  Run `aristo lint --fix` (or address manually) before deeper critique.
  Skipping deeper critique for this target until lint findings are resolved.

```
