# `aristo show --json` / `--toml` — structured output for tooling

Source: `../aretta-sdk/docs/mockups/06-cross-cutting-cli/examples.md` § F3 → "Structured output (for tooling / piping)".

Per F3-a: plain text by default, `--json` / `--toml` for piping. The structured shape mirrors `.aristo/index.toml` plus a compact `[children.*]` reverse-walk. For server-bound annotations the TOML form uses the quoted-key section header (`["aristos:..."]`) per the index schema (D1 + B5a-revised).

## JSON output (pipeable)

```console
$ aristo show aristos:balance_no_duplicate_cells --json
{
  "id": "aristos:balance_no_duplicate_cells",
  "kind": "intent",
  "verify": "full",
  "status": "verified",
[..]
}
```

## TOML output (compact, mirrors index schema)

```console
$ aristo show aristos:balance_no_duplicate_cells --toml
["aristos:balance_no_duplicate_cells"]
kind = "intent"
verify = "full"
status = "verified"
linked = "arta_[..]"
verified_outcome = "v1:[..]"
[..]

[children.cells_extracted_without_aliasing]
kind = "intent"
verify = "full"
status = "verified"
[..]
```
