# The vendored conductor route registry

`v0.2.239/` holds EIGHT files of `CampanileSkyForge/aretta-registry` tag `v0.2.239` (tag object 88f4b1ee →
commit f15d83cf8d3a474e25d453be863af186b3379ed5), taken from the registry's spec root `data/`, byte for byte:
`routes/registry.yaml` (the route registry: one row per conductor route, with its id, method, path and callers),
`VERSION`, and the generator that renders it with the modules it imports, `scripts/generate-routes.py`,
`scripts/registry_vocab.py`, `scripts/enum_schemas.py`, `scripts/layout.py`, `scripts/codes_vocab.py` and
`scripts/postable_vocab.py`. That is the smallest set that regenerates `paths.rs` identically to the registry's own
`data/generated/cli/` copy. They are never edited here; a new version is a new directory.

The pin is the directory name and its `.sha256` list. `VERSION` at this tag reads `0.2.238`; the generator reads it,
but nothing it writes for the CLI carries it, and nothing in the CLI reads it.

`crates/aristo-core/src/conductor/paths.rs` is what this generator writes for the CLI (the registry rows with
`callers: cli`). `scripts/check-conductor-paths.sh` regenerates it from this copy with the pinned rustfmt
(Rust 1.96.0's) and fails on any difference; CI runs it.

`v0.2.239.sha256` lists the eight files' sha256 (`sha256sum` format, paths relative to `v0.2.239/`). The test
`conductor::tests::the_vendored_spec_matches_its_sha256_list` holds the copy to it, so a local edit is caught.

To move to a newer registry tag: vendor the same files from its `data/` as a new `v<VERSION>/` with its `.sha256`,
point `scripts/check-conductor-paths.sh` and the lock test's `SPEC` at it, regenerate `paths.rs`, and delete the old
directory.
