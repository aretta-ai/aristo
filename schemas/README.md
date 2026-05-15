# `schemas/` — JSON Schemas for Aristo file formats

Canonical JSON Schemas (draft-07 via `schemars` 0.8) for every Aristo on-disk file format. **These are derived artifacts** — the source of truth is the Rust types in `aristo-core::index` (and, in future slices, the spec / config types).

| File | Source type | What it describes |
|---|---|---|
| `aristo-index.schema.json` | `aristo_core::index::IndexFile` | `.aristo/index.toml` — annotation metadata, hashes, server-binding state |
| _(slice 4)_ `aristo-spec.schema.json` | `aristo_core::spec::SpecFile` | `.aristo/specs/<id>.spec` — mined assertions for free-tier `verify = "test"` |
| _(slice 5)_ `aristo-config.schema.json` | `aristo_core::config::ConfigFile` | `aristo.toml` — project configuration |

## Regenerating after a Rust type change

```sh
cargo run --example dump-schemas
```

Then commit the regenerated file. The `tests/schemas.rs` integration test fails CI if any committed schema is out of date with the Rust types it was derived from — the diagnostic includes the first divergent line and tells you to re-run the example.

## Using a schema from another language

Aristo's K2 architecture has language SDKs shell out to the canonical Rust CLI (`aristo`) for protocol logic, so most other-language code does not need to parse Aristo files directly. When it does (e.g., a Python dashboard reading `.aristo/index.toml`, or a TS test runner inspecting `.aristo/specs/`), use a JSON-Schema-driven typed-codegen tool to produce idiomatic types in the target language. Suggested per-language tooling:

| Language | Tool | Output |
|---|---|---|
| Python | [`datamodel-code-generator`](https://github.com/koxudaxi/datamodel-code-generator) | Pydantic v2 models (with regex constraint enforcement) |
| TypeScript | [`json-schema-to-typescript`](https://github.com/bcherny/json-schema-to-typescript) | TypeScript interfaces + zod-style refinement helpers |
| Go | [`quicktype`](https://github.com/quicktype/quicktype) (`--lang go`) | Go structs with `json:"..."` tags |
| Rust (third-party consumer) | [`typify`](https://github.com/oxidecomputer/typify) | Idiomatic Rust types — note: the canonical types are already in `aristo-core` |

For pure structural validation without codegen, every major language has a JSON Schema validator (`jsonschema` in Python, `ajv` in JS, `jsonschema` crate in Rust, `gojsonschema` in Go).

## Why this lives at the workspace root

Per the K3 layout, the workspace root carries language-neutral artifacts (`skills/`, `docs/`, and now `schemas/`) alongside language-specific code under `crates/`. Other-language SDKs that join the project as sibling repos (`aristo-python`, `aristo-go`, etc., per K3) consume schemas from this location.

## Constraint coverage

The generated schemas include the structural constraints declared by the Rust types:

- Newtype `pattern` regexes for `Sha256`, `CommitHash`, `ArtaId`, `VerifiedOutcome`, `AnnotationId`
- Enum variants for `AnnotationKind`, `Status`, `VerifyMethod`, `CoveredRegion`
- Required/optional field shape via serde's `#[serde(default, skip_serializing_if = "Option::is_none")]`
- The kind-tagged `oneOf` union for `IndexEntry` (Intent vs Assume)

Schemas do NOT capture cross-key invariants like "id starting with `aristos:` requires the entry to have a `linked` field" — that lives in `IndexFile::validate()`. Other-language SDKs that need full validation must replicate this small post-parse check (or shell out to the CLI).
