# PHILOSOPHY.md — `aristo-authoring` skill

Distilled principles for writing `#[aristo::intent]` and `#[aristo::assume]` annotations. Each principle: one-line rule, brief rationale, links to case files in `./cases/` that exemplify it.

This file is the durable record of taste. It is **not** a status log, todo list, retrospective, or process narrative — that material belongs in case files (which are the audit trail) and in CLAUDE.md (which holds process).

---

## What an intent is FOR

An intent makes explicit *something that lives implicitly in the programmer's mind and is invisible from the code alone*:

- An invariant a refactor could subtly break without compile-error or test-failure feedback.
- A design choice that *looks* incomplete or wrong from outside, so an agent or new contributor might "fix" it and regress the system.
- Cross-cutting context an agent would otherwise reverse-engineer from tests, comments, or git archaeology.

The **content gate** runs before any style consideration: would a sharp reader of the code alone miss this? Could a plausible refactor break it silently? If both answers are no, don't write the intent. A perfectly-worded intent that fails the content gate still adds noise.

---

## P-SPEC-STYLE — prose with the precision of a spec, not the syntax of one

Write English sentences with precise domain nouns. State the invariant directly. No motivation prose ("so that…", "the way this works…"), no examples in the body, no narration. Avoid weasels ("usually", "typically", "by design"). Reserve normative keywords (MUST / MAY) for actual caller contracts.

Avoid the other extreme: formulas, regex, ∀-quantifiers, function-call syntax, code identifiers where domain nouns work. Those alienate the everyday reader and make intents brittle when names change. Likewise, don't anchor prose to **transient facts that falsify over time** — version-roadmap asides ("parked for v2"), dev-milestone references ("defers to commit 4"), or hardcoded counts ("the other five categories"). State the timeless invariant; the transient fact belongs in commit history or a tracking issue.

Cases: [text-hash-whitespace](./cases/2026-05-15-text-hash-whitespace.md), [body-hash-verbatim](./cases/2026-05-15-body-hash-verbatim.md), [sha256-from-bytes-canonical](./cases/2026-05-15-sha256-from-bytes-canonical.md), [critique-sweep-125](./cases/2026-06-29-critique-sweep-125-intents.md) (round audit; code-in-prose + transient-facts clusters).

---

## P-CHECK-TYPE-SYSTEM-FIRST — don't restate what the compiler enforces

Before writing an intent, ask whether Rust's type system already enforces the property: signature shape, exhaustive enum matching, trait bounds, lifetimes, `#[must_use]`. If yes, the intent is redundant — and usually misframes the failure mode (the author thinks something "could silently happen" when the compiler would have caught it).

Cases: [matches-filter-type-system](./cases/2026-05-15-matches-filter-type-system.md) (DELETE).

---

## P-NO-DOUBLE-INTENT — one annotation, one load-bearing invariant

If a rewrite reveals two distinct invariants in one body, split or move one. Mixed intents read as motivation prose and lose precision in both halves.

Exception: two claims that share one function AND are both about the same domain layer (e.g., both about file-system semantics of one write operation) can stay together if combining keeps the body tight.

Cases: [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md) (combined-not-split, exception), [file-copy-install-idempotent](./cases/2026-05-16-file-copy-install-idempotent.md) (caller-contract clause split off).

---

## P-INVARIANT-AT-LOAD-BEARING-SITE — annotate where the property is enforced

An invariant goes on the function that *enforces* it, not on every caller that *benefits from* it. Duplicating the same property across sites in a call chain creates noise and confuses the reader about which annotation is authoritative.

Cases: [snake-case-from-text-delete](./cases/2026-05-15-snake-case-from-text-delete.md) (system invariant moved to enforcement site), [index-atomic-duplicate](./cases/2026-05-15-index-atomic-duplicate.md) (atomicity belongs on `atomic_write`, not on the caller; DELETE), [critique-sweep-125](./cases/2026-06-29-critique-sweep-125-intents.md) (the 8 silent-staleness scope findings — claims stapled to a site they can't drift).

---

## P-INVARIANT-NOT-IMPL — annotate properties the type system can't express

Don't restate what `-> Option<T>` already signals ("returns None on some inputs"). The annotation should add information beyond what's visible in the signature. The exact predicate for *when* None is returned is usually implementation detail unless the predicate itself is load-bearing for callers.

Cases: [snake-case-from-text-delete](./cases/2026-05-15-snake-case-from-text-delete.md).

---

## P-WHY-AS-INVARIANT — "why" is allowed *only* when the design choice IS the invariant

"Why" prose as motivation ("so that lint reformatting doesn't invalidate stamps…") is filler — the rule itself is the spec; the motivation belongs in commit history.

"Why" prose as load-bearing design content ("a low-entropy id silently committed would be worse than a failed run the user can retry") IS the invariant — that's the choice a refactor would reverse without realizing the implication.

Test for which: if the "why" content is itself the thing a refactor could subtly break, keep it. If it just explains motivation a reader could infer, cut it.

Cases: [generate-opaque-id-panic](./cases/2026-05-15-generate-opaque-id-panic.md), [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md), [freshness-check-one-shot](./cases/2026-05-16-freshness-check-one-shot.md).

---

## P-NAME-THE-REFACTOR-TRAP — name the likely-bad refactor in the body

When the invariant exists *because* a plausible-but-misguided refactor instinct would break it, name the refactor in the intent body. "Sorting or hashing the result would silently break X." "Parallelism would silently break Y." "Returning Result here would silently let weak entropy through."

This speaks the language of the change a future reader is about to make. The agent proposing the change sees their own proposal in the intent and stops.

Cases: [extract-source-order](./cases/2026-05-15-extract-source-order.md), [walk-determinism](./cases/2026-05-15-walk-determinism.md), [stamp-check-never-writes](./cases/2026-05-15-stamp-check-never-writes.md), [bundled-skills-stable-set](./cases/2026-05-16-bundled-skills-stable-set.md).

---

## P-AGENT-PROOFING — "intentional, not incomplete" when design stops short

Agents and new programmers default to "let me complete this" or "let me make this consistent." When a design deliberately stops short of what looks like the obvious next step (one cycle reported vs. all cycles, no Result on a panic-on-failure function), say *intentional, not incomplete* explicitly — the literal phrase, or one like it. Costs three words; prevents an entire class of well-intentioned regressions.

Cases: [cycle-first-only](./cases/2026-05-15-cycle-first-only.md).

---

## P-VERIFY-MATCHES-SHAPE — verify level tracks the load-bearing claim's shape

Pick the `verify` level based on the *verifiability shape of the load-bearing claim*, not the importance of the intent or the testability of side claims.

| Load-bearing claim is… | `verify =` |
|---|---|
| Runtime property a mined assertion or test can catch | `"test"` |
| Design decision / refactor-trap / "intentional, not incomplete" — reviewable by reading code, not reducible to a runtime check | `"neural"` |
| Formal-proof candidate (algorithmic invariant amenable to a solver) | `"full"` |
| Pure coordination convention with no checkable shape | `false` |

Over-marking design-philosophy intents as `"test"` is dishonest — no test will ever be derived, so it pollutes the verification pipeline with permanently-unverifiable entries. Under-marking testable invariants as `"neural"` wastes the testing pipeline's stronger signal.

**Verify-level re-check after writing the test.** When intent and test land in the same commit, the intent is usually written *first* (per concurrent-authoring discipline) and defaults to `"neural"` because the test doesn't exist yet at intent-write time. After the test is written, re-read the most-recently-added intent at that site: if the test fires on the intent's load-bearing claim, shift to `"test"`. Skipping this re-check is the dominant cause of under-marking — round 3 caught 4 such cases in slices 19–23.

P-WHY-AS-INVARIANT and P-VERIFY-MATCHES-SHAPE are coupled: any intent whose body relies on "why" content to be load-bearing is probably a `"neural"` intent, not a `"test"` intent.

Cases: [generate-opaque-id-panic](./cases/2026-05-15-generate-opaque-id-panic.md), [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md), [did-you-mean-threshold](./cases/2026-05-15-did-you-mean-threshold.md), [bundled-skills-stable-set](./cases/2026-05-16-bundled-skills-stable-set.md), [install-skills-scope-symmetry](./cases/2026-05-16-install-skills-scope-symmetry.md).

---

## P-ROOT-CAUSED-BUG-IS-A-SPEC-CASE — encode subtle, surprising bugs as checkable intents

A bug that took non-trivial debugging to root-cause is high-signal: the reason it was missed in the first place is usually that the underlying spec / case was subtle, implicit, or open-ended. The fix patches *one instance*; the intent locks in the *invariant the bug just demonstrated must hold*. Without it, a plausible future refactor regresses to the same failure mode and the same debugging spend pays out twice.

The intent goes on the load-bearing site of the fix — the function, branch, or design choice that the bug taught us cannot be casually altered. Its body should name the failure mode in the language a reviewer would use, and where applicable name the refactor that re-introduces the bug (this overlaps with P-NAME-THE-REFACTOR-TRAP). Pair it with a regression test that asserts the new behavior, so `verify="test"` is the natural level.

When to invoke this principle:

- The bug took meaningful debugging time (not a one-grep find).
- The user pair-debugged with you.
- The fix is more than a typo or a missing import — there's a *design lesson* in the post-mortem.
- The fix changes a default, swaps a fragile mechanism for a robust one, or expands a case-list — any of which can silently re-narrow in a future cleanup.

When NOT to invoke it:

- Surface-level bugs: typo, off-by-one inside an existing-and-tested function, a misnamed variable. The fix and its test are sufficient; an intent here is noise.
- Bugs whose fix is self-evidently the right behavior from the function signature alone (P-CHECK-TYPE-SYSTEM-FIRST applies).

The intent is the durable artifact; the test is the mechanical guard. The two together convert what would otherwise be tribal-knowledge ("oh yeah, we hit that years ago") into a checkable invariant.

Cases: [stmt-form-visit-descent](./cases/2026-05-16-stmt-form-visit-descent.md) (whitelist-of-Expr-variants dropped `intent_stmt!` inside `match` arms; root cause was the open-ended Expr enum vs. a closed hand-rolled list).
