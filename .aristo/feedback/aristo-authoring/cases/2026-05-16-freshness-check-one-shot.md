---
date: 2026-05-16
slice: 19
file: crates/aristo-cli/src/preflight.rs:39
id: freshness_check_compares_source_mtimes_to_index
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-WHY-AS-INVARIANT, P-VERIFY-MATCHES-SHAPE]
verify_was: neural
verify_is: neural
---

## Original (v0)

> If any source file's mtime is newer than the index file's mtime, the index is stale relative to source. The comparison is one-shot per command invocation; no caching, no incremental tracking — correctness over speed for an advisory check.

## Better (v2)

> Recomputed from scratch on every invocation — no caching, no incremental tracking between calls. Correctness over speed: an advisory check shouldn't introduce its own cache-staleness mode.

## Why the gap

v0's first sentence is an operational *definition* of staleness — the function name `freshness_check` and its `FreshnessReport` return type already say what it does. The load-bearing content is the no-cache design choice and its rationale, which v2 leads with. Per P-WHY-AS-INVARIANT, the "correctness over speed" framing IS the invariant a refactor would reverse ("let's cache per-file mtimes between invocations for perf").

## Verify level

- was: `neural`
- is: `neural`
- reason: the load-bearing claim is a design judgment about caching strategy, not a runtime property. The mtime comparison itself is testable, but the "no caching" choice is reviewable by reading code, not by mining a runtime assertion.

## Round-2 backfill note

Slice 19 shipped this intent without surfacing through the §10A reflection loop. Caught during the milestone-D backfill round 2026-05-16 (user prompt: "did you surface the new annotations from the last phase for feedback"). Going forward, §10A reflection runs at slice close, not only at milestone close — paired with §12A's slice-startup-protocol.
