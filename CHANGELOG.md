# Changelog

All notable changes to Aristo are recorded here. **Every commit adds one bullet** to the `## [Unreleased]` section, in customer-facing language. At release time, `[Unreleased]` is promoted to a versioned section and should read coherently as a release-note draft.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Pre-`0.1.0` is the initial bootstrap series.

See [`CLAUDE.md`](./CLAUDE.md) §3 for the discipline.

## [Unreleased]

### Added
- repo: initial repository scaffold — LICENSE (MIT), README, `.gitignore`, this CHANGELOG, and `CLAUDE.md` working agreement.
- build: four-crate Cargo workspace (`aristo`, `aristo-core`, `aristo-macros`, `aristo-cli`) per K3 — empty skeletons; `cargo fmt`/`check`/`clippy -D warnings`/`test` all green. CLI binary stubbed to exit 1 with "not yet implemented".
