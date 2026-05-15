# `aristo show --json` / `--toml` — structured output for tooling

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "Structured output (for tooling / piping)".

Per F3-a: plain text by default, `--json` / `--toml` for piping. The structured shape mirrors `.aristo/index.toml` plus a compact `[children.*]` reverse-walk.

## JSON output (pipeable)

```console
$ aristo show balance_no_duplicate_cells --json
{
  "id": "balance_no_duplicate_cells",
  "kind": "intent",
  "verify": "full",
  "status": "verified",
[..]
}
```

## TOML output (compact, mirrors index schema)

```console
$ aristo show balance_no_duplicate_cells --toml
[balance_no_duplicate_cells]
kind = "intent"
verify = "full"
status = "verified"
[..]

[children.cells_extracted_without_aliasing]
kind = "intent"
verify = "full"
status = "verified"
[..]
```
