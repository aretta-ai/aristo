# Changelog

All notable changes to Aristo are recorded here. **Every commit adds one bullet** to the `## [Unreleased]` section, in customer-facing language. At release time, `[Unreleased]` is promoted to a versioned section and should read coherently as a release-note draft.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Pre-`0.1.0` is the initial bootstrap series.

See [`CLAUDE.md`](./CLAUDE.md) §3 for the discipline.

## [Unreleased]

### Added
- repo: initial repository scaffold — LICENSE (MIT), README, `.gitignore`, this CHANGELOG, and `CLAUDE.md` working agreement.
- build: four-crate Cargo workspace (`aristo`, `aristo-core`, `aristo-macros`, `aristo-cli`) per K3 — empty skeletons; `cargo fmt`/`check`/`clippy -D warnings`/`test` all green. CLI binary stubbed to exit 1 with "not yet implemented".
- test: integration test harness for the CLI — `trycmd` for declarative `console`-fenced session scenarios (sourced from `docs/mockups/`), `assert_cmd` + `predicates` for imperative tests. Convention: `tests/cmd/active/*.md` runs and must pass; `tests/cmd/_pending/*.md` is the parking lot for unimplemented surface, moved into `active/` in the same commit that lands each command. Smoke test asserts the current stub binary's "exit 1 / not yet implemented" behavior.
- docs: `docs/TESTING.md` — testing convention covering toolchain (trycmd / assert_cmd / trybuild), the `_pending/` → `active/` promotion rule, mockup-to-scenario conversion recipes, scenario naming, and the trycmd assertion-model quick reference.
