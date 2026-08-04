**Aristo verified intent — `drifted_files_reports_missing_as_drift`**

Returns the names (from `expected`) whose on-disk bytes under `out` differ from the freshly-rendered content — the regenerate-and-compare core of every `--check` drift guard. A MISSING file counts as drifted: an absent artifact is never 'up to date' (read errors map to an empty string, which differs from any non-empty rendered content). Shared by `gen-c --check` (renders from the SUT's directives) and `vendor-c --check` (renders from the CLI templates) — the render source differs, this comparison does not.

<sub>Verify level: **test**</sub>

---
