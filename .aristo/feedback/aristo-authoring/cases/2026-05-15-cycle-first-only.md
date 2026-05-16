---
date: 2026-05-15
slice: 15
file: crates/aristo-core/src/cycle.rs:52
id: detect_cycles_returns_first_cycle_only
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-AGENT-PROOFING, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
---

## Original (v0)

> detect_cycles returns the FIRST cycle it finds and stops; it does not enumerate all cycles in the graph. The diagnostic-friendly path is enough for the user to break the cycle and re-run; chasing every cycle on the same pass would multiply diagnostic noise without helping the fix.

## Better (v2)

> One cycle reported per call, then return. This is intentional, not incomplete — extending to enumerate all cycles would multiply diagnostic noise without helping the fix.

## Why the gap

v0 uses adjective filler ("diagnostic-friendly") and narrates the user workflow ("is enough for the user to break the cycle and re-run"). v2 leads with the rule, then says **"intentional, not incomplete"** explicitly (P-AGENT-PROOFING) — this is the textbook case where a function looks unfinished from the outside (returns one of many) and an agent would propose "let me complete this" without realizing the design choice. The three-word phrase prevents that whole class of regression.

## Verify level

- was: `test`
- is: `neural`
- reason: load-bearing claim is "this is intentional, not incomplete" — a design judgment. You could test "given a graph with multiple cycles, returns exactly one," but that's a thin property; the meaningful invariant is the design intent. Per P-VERIFY-MATCHES-SHAPE: design judgments → `neural`.
