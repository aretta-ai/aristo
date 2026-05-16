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

Avoid the other extreme: formulas, regex, ∀-quantifiers, function-call syntax, code identifiers where domain nouns work. Those alienate the everyday reader and make intents brittle when names change.

Cases: [text-hash-whitespace](./cases/2026-05-15-text-hash-whitespace.md), [body-hash-verbatim](./cases/2026-05-15-body-hash-verbatim.md), [sha256-from-bytes-canonical](./cases/2026-05-15-sha256-from-bytes-canonical.md).

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

Cases: [snake-case-from-text-delete](./cases/2026-05-15-snake-case-from-text-delete.md) (system invariant moved to enforcement site), [index-atomic-duplicate](./cases/2026-05-15-index-atomic-duplicate.md) (atomicity belongs on `atomic_write`, not on the caller; DELETE).

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

P-WHY-AS-INVARIANT and P-VERIFY-MATCHES-SHAPE are coupled: any intent whose body relies on "why" content to be load-bearing is probably a `"neural"` intent, not a `"test"` intent.

Cases: [generate-opaque-id-panic](./cases/2026-05-15-generate-opaque-id-panic.md), [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md), [did-you-mean-threshold](./cases/2026-05-15-did-you-mean-threshold.md), [bundled-skills-stable-set](./cases/2026-05-16-bundled-skills-stable-set.md), [install-skills-scope-symmetry](./cases/2026-05-16-install-skills-scope-symmetry.md).
