---
date: 2026-06-29
round: critique-sweep — all 125 dogfooded intents (`aristo critique --all`)
verdict: principles-confirmed (enforcement gap, not a documentation gap)
principles: [P-SPEC-STYLE, P-INVARIANT-AT-LOAD-BEARING-SITE, P-NO-DOUBLE-INTENT, P-WHY-AS-INVARIANT]
outcome: 28 clean, 97 with findings (119 findings — 91 rephrasing, 8 scope, 5 clarity; 21 strong-suggest)
---

## What this round was

A one-shot critique sweep over every dogfooded `#[aristo::intent]` in the SDK (`aristo critique --all`, 8 workers, this PHILOSOPHY.md as the rubric). Most of these intents were hand-written before the authoring skill matured, so the round is primarily a backfill audit of pre-skill prose against the current principles.

## Outcome: the principles held; the gap is enforcement

Every recurring finding mapped to an **existing** principle — no new P-tag was needed (adding one would be patchwork). The clusters:

| Cluster | ~count | Principle it violates |
|---|---|---|
| Code / identifier / formula in prose | 91 | P-SPEC-STYLE |
| Claim anchored where it can go silently stale | 8 | P-INVARIANT-AT-LOAD-BEARING-SITE |
| Two invariants in one body | several | P-NO-DOUBLE-INTENT |
| Motivation / consumer narration ("uses it to…", "lets us…") | several | P-WHY-AS-INVARIANT |
| Identifier rename-fragility | several | P-SPEC-STYLE |

Takeaway for the loop: the philosophy is not under-specified — the dogfood corpus simply predates it. The remediation is to *apply* the existing principles (triage + rewrite the 97 flagged intents), not to document more. The 28 clean intents were all authored to the bar.

## Two representative cases (real text from this round)

### Precision contradiction — `drain_returns_items_then_deletes_file` (P-SPEC-STYLE, strong-suggest)

v0 opened "removes the file after returning its contents" but closed with read-then-delete — contradicting the very ordering that is the point of the annotation. Better: "reads every item into memory, then deletes the file before returning — so the caller always holds the items while the file is already gone," with the two named refactor traps (zombie-deferral on read-without-delete; lost-on-crash on delete-before-read). Lesson: when ordering *is* the invariant, the prose must not contradict it; also dropped the "atomically" overstatement (it is an ordering guarantee, not atomicity).

### Silent-staleness scope — `verify_queue_status_is_non_destructive_peek` (P-INVARIANT-AT-LOAD-BEARING-SITE, strong-suggest)

The body stapled the verify-orchestrator's "one-shot workers don't loop" policy onto a function that only prints queue counts. Doubly mis-scoped: a change to the worker policy never drifts this body (silent staleness), and an unrelated edit to the print fn needlessly re-pends a worker-lifecycle claim. The worker-loop rationale belongs on the dispatch/orchestrator site. All 8 scope findings are this shape — the one cluster with *verification* consequences: a proof can read fresh while the real mechanism drifts.

## The one sliver not previously covered

A handful of intents anchored prose to **transient facts that falsify over time** — version-roadmap asides ("parked for v2", "revisit after first month"), dev-milestone references ("defers all writes to commit 4"), and hardcoded counts ("the other five categories"). P-SPEC-STYLE covered identifier *renames* but not the time-staleness of stated facts. Captured as a one-line clause on P-SPEC-STYLE, not a new principle.

## Not done in this round

Remediation (applying the 119 findings to the source) is a separate triage/apply task, not part of this reflection. Verify-level re-checks (P-VERIFY-MATCHES-SHAPE) were out of scope for a prose-only critique pass.
