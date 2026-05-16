---
date: 2026-05-15
slice: 14C
file: crates/aristo-core/src/walk/fs.rs:58
id: walk_directory_is_deterministic
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-NAME-THE-REFACTOR-TRAP]
verify_was: test
verify_is: test
---

## Original (v0)

> walk_directory returns paths RELATIVE to root, in stable lexicographic order — same input directory must yield byte-identical output across runs and across machines so .aristo/index.toml stays deterministic.

## Better (v2)

> The same source tree yields byte-identical results across runs and machines: lexicographic path order, source order within each file. Parallelism or unsorted directory reads would silently break the index's reproducibility guarantee.

## Why the gap

v0 mixes a type-visible claim ("paths RELATIVE to root") with the load-bearing implicit one (cross-run + cross-machine byte-identity). v2 drops the type-visible piece and names the refactor traps explicitly: `par_iter` without final sort, switching to `std::fs::read_dir` (unsorted), sorting by mtime instead of name (P-NAME-THE-REFACTOR-TRAP).

## Verify level

- was: `test`
- is: `test`
- reason: within-process determinism is testable (run walk twice on the same input, assert equal). Cross-machine determinism is transitive from the algorithm shape and tested implicitly by CI running on multiple platforms.
