---
date: 2026-05-15
slice: 16
file: crates/aristo-cli/src/commands/index.rs:36
id: aristo_index_writes_atomically
verdict: delete
principles: [P-INVARIANT-AT-LOAD-BEARING-SITE]
verify_was: test
verify_is: (deleted)
---

## Original (v0)

> aristo index writes .aristo/index.toml ATOMICALLY: temp file in the same directory + rename. A crash mid-write leaves the previous index intact rather than a half-formed one — `aristo show` / `aristo list` / etc. always see a consistent file or the prior version, never a parser error from a truncated rewrite.

## Better

(none — annotation deleted from source)

## Why deleted

The atomicity invariant lives on `atomic_write()` (case [atomic-write-tempfile](./2026-05-15-atomic-write-tempfile.md)) — that's the load-bearing site (P-INVARIANT-AT-LOAD-BEARING-SITE). This annotation on the outer `run()` function duplicates a property already locked in at a more specific site, adding noise without coverage.

The `run()` body is orchestration (walk → build → cycle-check → write); each step is annotated at its own site. The "downstream consumers see a consistent file" framing is a *consumer-side observation*, not a property of `run()` itself.

## Verify level

n/a (deleted).
