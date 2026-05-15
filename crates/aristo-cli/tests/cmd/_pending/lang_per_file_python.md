# `aristo lang --file <path>` — per-file detection in mixed-language repos

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K5 → Per-file query (for mixed-language repos)".

`--file <path>` makes language detection per-call rather than per-repo, so a polyglot monorepo (Rust crates + Python services + Go binaries sharing a single `.aristo/index.toml`) can authoritatively name the right syntax for each file.

This scenario will activate once the Python `LanguageSyntax` impl lands (Phase 2+). Until then, the CLI errors with the supported-languages list (see `lang_unsupported.md`).

```console
$ aristo lang --file scripts/setup.py
Detected language: Python (.py extension; pyproject.toml present)

# Aristo annotation syntax — Python

## Decorator form (function / class / method)
  @aristo.intent("text here", verify="test", id="snake_case_id", parent="other_id")
  def the_thing(): ...

## Inline form (for sub-statement: before a loop or block)
  aristo.intent("text here", verify="test")
  for item in items:
      ...

## Assume (no verify field)
  @aristo.assume("OS guarantee or library invariant")
  def the_thing(): ...

## Parent linkage
  parent="balance_no_duplicate_cells"
  parent=["a", "b"]

## Verify levels (same vocabulary as Rust)
  False | "neural" | "test" | "full" | True

For full reference: https://aristo.ai/docs/lang/python
```
