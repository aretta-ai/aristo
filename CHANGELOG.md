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
- test: `_pending/` scenarios for cross-cutting CLI commands (mockup 06 — F1/F2/F3) — 9 trycmd files covering `aristo rename` (dry-run, actual run, three error paths), `aristo stamp` cycle diagnostics (multi-node cycle, self-cycle, diamond-is-DAG), and `aristo show` (by id, by function name with multi-match disambiguation, by file:line, error paths with did-you-mean / stale-index / empty-result, JSON+TOML structured output).
- test: aligned mockup-06 `_pending/` scenarios to current B5a-revised + B5b + K1 schema — server-bound annotations carry the `aristos:` prefix in source ids and parent references, the per-annotation `sig` field is replaced by `verified_outcome` (Ed25519, `v1:...` form), `linked` opaque uses the `arta_<base32>` form, and `rename_error_cases.md` adds a cross-namespace-rejection scenario per the TOOLS.md rename rule. Mirrors the `aretta-sdk` mockup update commit `49970ea`.
- test: `_pending/` scenarios for `aristo lang` (mockup 12 — K5) — 3 trycmd files covering Rust auto-detection from `Cargo.toml` (full cheat-sheet output), per-file detection via `--file scripts/setup.py` for mixed-language repos (Python; activates with the Phase 2+ Python `LanguageSyntax` impl), and the unsupported-language error path that lists supported + planned languages.
- test: `_pending/` scenarios for `aristo install-skills` + `aristo uninstall-skills` (mockup 12 — K4) — 10 trycmd files covering the two install models (file-copy: `claude-code`, `cursor`, `antigravity`; AGENTS.md section-injection: `codex`, `opencode` reusing the codex block), the `--list-agents` enumerator, the `--update` no-op-when-current path, the `--user` cross-project install, and uninstall for both install models (AGENTS.md section strip; file removal with locally-modified-skip + `--force` advisory).
