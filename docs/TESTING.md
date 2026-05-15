# Testing in Aristo

This document is the testing convention for the `aristo` repo. It complements [`../CLAUDE.md`](../CLAUDE.md) §4 (test-first) and §6 (every commit passes the full check suite).

## Why this convention exists

Phase 0 (in the parent `aretta-sdk/` design archive) produced ~12 directories of mockups under `../aretta-sdk/docs/mockups/` plus workflow diagrams under `../aretta-sdk/docs/diagrams/`. These define **the behavior we are committing to ship**. We turn them into executable tests so:

1. Every CLI command's documented output is the spec it must satisfy.
2. Drift between docs and implementation is impossible — the docs ARE the tests.
3. Test-first per CLAUDE.md §4: the test exists before we write the code.

## Toolchain

| Tool | Purpose | Where used |
|---|---|---|
| **trycmd** (0.15) | Declarative `console`-fenced CLI session scenarios; reads almost like the mockups already do | `crates/aristo-cli/tests/cmd/active/*.md` |
| **assert_cmd** + **predicates** (2.0 / 3.1) | Imperative tests where setup is complex (temp git repo, mtime manipulation, multi-step state) | `crates/aristo-cli/tests/*.rs` |
| **trybuild** (added when `aristo-macros` lands its first macro) | Proc-macro compile-pass / compile-fail tests | `crates/aristo-macros/tests/` |
| Stock `#[test]` unit tests | Pure logic, parsers, format round-trips | each crate's `src/` (`#[cfg(test)] mod tests`) |

We considered `testscript-rs` (a Go-testscript port). Rejected: niche in Rust, far smaller ecosystem than the Ed-Page-maintained trycmd/assert_cmd/snapbox stack used by `cargo`, `clap`, `ripgrep`, `fd`, `bat`.

## Directory layout

```
crates/aristo-cli/tests/
├── binary_smoke.rs           # assert_cmd: harness canary
├── cli_scenarios.rs          # trycmd runner (globs active/*.md)
├── cmd/
│   ├── active/               # scenarios for IMPLEMENTED commands; MUST pass
│   │   └── *.md
│   └── _pending/             # scenarios for unimplemented commands; not run
│       └── *.md
└── (future: setup_*.rs for assert_cmd-style imperative tests)
```

`crates/aristo-core/tests/` and `crates/aristo-macros/tests/` follow the same shape when they grow tests.

## The `_pending/` convention

Every commit must pass `cargo test --workspace` (CLAUDE.md §6). But TDD per §4 says the test exists before the code. These goals collide for unimplemented CLI commands.

Resolution: scenarios for not-yet-implemented commands live in `tests/cmd/_pending/`, which the trycmd glob does NOT pick up. They are real test files, version-controlled, reviewable — but inert until promoted.

**Promotion rule (NON-NEGOTIABLE):** the commit that lands command `X` is also the commit that `git mv`'s the corresponding scenario file from `_pending/` to `active/`. There is no separate "enable tests" commit.

## Mockup-to-scenario conversion

### CLI session mockups → trycmd `.md` files

Mockup source (e.g., `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md`):

````markdown
```
$ aristo lang
Detected language: Rust (from Cargo.toml at ...)
...
```
````

Becomes a trycmd scenario at `crates/aristo-cli/tests/cmd/_pending/lang_detect_rust.md`:

````markdown
# `aristo lang` — Rust auto-detection

Source: `docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K5 — `aristo lang` for the current repo".

```console
$ aristo lang
Detected language: Rust (from Cargo.toml at [..])
...
```
````

Notes on the transform:
- ` ``` ` → ` ```console ` (trycmd fence language).
- Volatile values (paths, timestamps, hashes) → `[..]` wildcard.
- Each mockup section (typically one `$` invocation) becomes one `.md` file. File name is `<command>_<scenario>.md` in snake_case.
- Top-of-file comment cites the mockup source so the trail back to design rationale is one click away.

### Format-and-data mockups → unit tests

Mockups like `05-index-and-indexer/sample.toml` and `04-staleness/sample.spec` describe on-disk file formats. These become:

- A fixture file under `crates/aristo-core/tests/fixtures/`.
- A unit test that round-trips: parse → serialize → byte-equal (or normalize-then-equal where ordering is unspecified).
- Plus one negative test per documented invalid case.

### Source-syntax mockups → trybuild

Mockups under `01-surface-syntax/` and `02-non-fn-targets/` show macro inputs that should compile or fail to compile. These become trybuild fixtures under `crates/aristo-macros/tests/ui/` once the macros crate has its first proc-macro export.

### Workflow diagrams → multi-step trycmd scenarios

Diagrams in `../aretta-sdk/docs/diagrams/*.mmd` show state transitions across multiple commands. Each diagram becomes one scenario file walking the full sequence:

````markdown
# Lifecycle: init → annotate → stamp → verify

Source: `docs/diagrams/01-lifecycle.mmd`.

```console
$ aristo init
...

$ aristo stamp
...

$ aristo verify
...
```
````

## Scenario file naming

`<command>_<short_scenario>.md`. Examples:

- `init_fresh_repo.md`
- `stamp_assigns_aret_hash.md`
- `verify_full_downgrades_on_free_tier.md`
- `rename_dry_run_preview.md`
- `lang_detect_rust.md`

Hyphens are allowed but underscores match the rest of our snake_case naming (annotation ids, etc.).

## The trycmd assertion model — quick reference

````markdown
```console
$ aristo init
ok: created aristo.toml, .aristo/, pre-commit hook installed.

$ aristo init
? 1
error: aristo already initialized in this directory.
```
````

- Lines beginning `$ ` are commands.
- Optional `? N` on the next line specifies expected exit code (default 0).
- Subsequent non-`$` lines are expected stdout (and stderr — they're merged unless split with `[stdout]` / `[stderr]` markers).
- `[..]` matches arbitrary content within a line; `...` on its own line matches any number of intervening lines.
- Re-run with `TRYCMD=overwrite cargo test` to auto-update expected output during legitimate behavior changes — but ONLY after eyeballing the diff.

Full reference: <https://docs.rs/trycmd>.

## Invariants

1. `cargo test --workspace` is green on every commit (CLAUDE.md §6).
2. A scenario in `active/` that fails blocks the commit; fix the code or fix the scenario in the same change.
3. Adding a new `_pending/` scenario does NOT require backing implementation — it's a TDD-red test in waiting.
4. Removing a scenario without explanation is forbidden; either it migrates to `active/` (because the feature shipped) or it's deleted in a `docs:`/`test:` commit explaining why the documented behavior is no longer the spec.
