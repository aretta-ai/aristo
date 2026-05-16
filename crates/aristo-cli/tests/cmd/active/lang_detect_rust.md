# `aristo lang` — auto-detect Rust from `Cargo.toml`

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K5 — `aristo lang` for the current repo".

With no arguments, `aristo lang` detects the repo's primary language from manifest files (`Cargo.toml`, `pyproject.toml`, `go.mod`, ...) and emits a compact, agent-readable syntax cheat sheet. Phase 1 ships Rust only; per K5, adding language N = implementing `LanguageSyntax` + registering — skills untouched.

```console
$ aristo lang
Detected language: Rust (from Cargo.toml at [..]/Cargo.toml)

# Aristo annotation syntax — Rust

## Attribute form (preferred for fn / struct / impl / trait / mod / type / field / variant)
  #[aristo::intent("text here", verify = "test", id = "snake_case_id", parent = "other_id")]
  fn the_thing() { ... }

## Function-like form (sub-item: before a statement / loop / block)
  aristo::intent_stmt!("text here", verify = "test");
  for item in items { ... }

## Assume (no verify field; states external invariants you rely on)
  #[aristo::assume("OS guarantee or library invariant")]
  fn the_thing() { ... }

## Parent linkage (singular or list)
  parent = "balance_no_duplicate_cells"
  parent = ["a", "b"]

## Verify levels
  false      | documentation only; no check ever runs
  "neural"   | AI-reasoned property check
  "test"     | mined assertions + existing test suite
  "full"     | server formal proof attempt (paid tier)
  true       | resolves to project default in aristo.toml [verify] default_method

## Namespace prefix
  `aristos:` prefix appears on id after `aristo sync` binds the annotation
  to the Aristo server. NEVER write `aristos:` manually.

## Cargo features (consumer-side)
  aristo_verify | injects mined assertions during `aristo verify --filter ...`
  aristo_check  | compile-time per-annotation validation
  aristo_doc    | rustdoc integration via include_str!

For full reference: https://aristo.ai/docs/lang/rust

```
