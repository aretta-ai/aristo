# PHILOSOPHY.md — `aristo-authoring` skill

Distilled principles for writing `#[aristo::intent]` and `#[aristo::assume]`
annotations. The skill loads this file alongside its `SKILL.md` body (wiring
lands in milestone D, task #37). Until then, this file is the human + skill
reference; the authoring agent uses these principles when writing intents on
Aristo source and on downstream user code.

**Structure** (modeled on Rust API Guidelines / OpenAI Model Spec / Chicago
Manual of Style): each principle has a one-line rule, a rationale paragraph,
and links to case files (`./cases/<date>-<slug>.md`) that exemplify it.
Anti-patterns first — most authoring drift comes from these.

**Status:** draft. First reflection round, 2026-05-15, distilled from 15
intents written across slices 14–18. Three intents were deleted in the
reflection (3 of 15 = 20% miss rate); each delete became a case showing the
anti-pattern that justified it.

---

## Calibration: what an intent is FOR

An intent makes explicit *something that lives implicitly in the
programmer's mind and is invisible from the code alone* — typically:

- An invariant a refactor could subtly break without compiling-error or
  test-failure feedback.
- A design choice that *looks* incomplete or wrong from outside, so an
  agent / new contributor might "fix" it and regress the system.
- Cross-cutting context an agent would otherwise reverse-engineer from
  tests, comments, or git archaeology.

Before writing, apply the **content gate**: would a sharp reader of the
code alone miss this? Could a plausible refactor break it silently? If
both answers are no, do not write the intent.

The content gate runs *before* style. A perfectly-worded intent that
fails the gate still adds noise; a load-bearing intent in v0 prose is
still load-bearing.

---

## P-SPEC-STYLE — prose with the precision of a spec, not the syntax of one

Write intents as English sentences with precise nouns. State the
invariant directly. No motivation prose ("so that…", "the way this
works…"), no examples in the body, no narration. Avoid weasels
("usually", "typically", "by design"). Reserve normative keywords (MUST
/ MAY) for actual caller contracts.

Avoid the other extreme: formulas, regex, ∀-quantifiers, function-call
syntax, and naming exact identifiers where domain nouns work. Those
alienate the everyday reader and make intents brittle (rename a
function, the intent rots).

The target shape is closer to POSIX man pages, W3C normative language,
Postgres documentation, or TigerBeetle's design docs than to a Coq
lemma.

Cases: [text-hash-whitespace](./cases/2026-05-15-text-hash-whitespace.md),
[body-hash-verbatim](./cases/2026-05-15-body-hash-verbatim.md),
[sha256-from-bytes-canonical](./cases/2026-05-15-sha256-from-bytes-canonical.md).

---

## P-CHECK-TYPE-SYSTEM-FIRST — don't restate what the compiler enforces

Before writing an intent, ask whether Rust's type system already
enforces the property: signature shape, exhaustive enum matching,
trait bounds, lifetimes, `#[must_use]`. If yes, the intent is
redundant — and worse, often misframes the failure mode (the author
thinks something "could silently happen" when the compiler would have
caught it).

This is the strictest form of P-INVARIANT-NOT-IMPL. An exhaustive
`match` over a closed enum cannot silently omit an arm; saying it can
is factually wrong.

Cases: [matches-filter-type-system](./cases/2026-05-15-matches-filter-type-system.md)
(DELETE).

---

## P-NO-DOUBLE-INTENT — one annotation, one load-bearing invariant

If a rewrite reveals two distinct invariants in one body, split or move
one. Mixed intents read as motivation prose and lose precision in both
halves. Exception: two claims that share a function AND are both about
the same domain layer (e.g., file-system semantics of one write
operation) can stay together if combining keeps the body tight.

Cases: [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md)
(combined-not-split; both claims about file-system semantics of one
call).

---

## P-INVARIANT-AT-LOAD-BEARING-SITE — annotate where the property is enforced

An invariant goes on the function that *enforces* it, not on every
caller that *benefits from* it. Duplicating the same property across
sites in a call chain creates noise and confuses the reader about
which annotation is authoritative.

Cases: [snake-case-from-text-delete](./cases/2026-05-15-snake-case-from-text-delete.md)
(moved the "every annotation gets an id" claim to its enforcement
site), [index-atomic-duplicate](./cases/2026-05-15-index-atomic-duplicate.md)
(DELETE; atomicity belongs on `atomic_write`, not on the caller).

---

## P-INVARIANT-NOT-IMPL — annotate properties the type system can't express

Don't restate what `-> Option<T>` already signals ("returns None on
some inputs"). The annotation should add information beyond what's
visible in the signature. The exact predicate for *when* None is
returned is usually implementation detail unless the predicate itself
is load-bearing for callers.

Cases: [snake-case-from-text-delete](./cases/2026-05-15-snake-case-from-text-delete.md).

---

## P-WHY-AS-INVARIANT — "why" is allowed *only* when the design choice IS the invariant

"Why" prose as motivation ("so that lint reformatting doesn't
invalidate stamps…") is filler — the rule itself is the spec; the
motivation belongs in commit history.

"Why" prose as load-bearing design content ("a low-entropy id silently
committed would be worse than a failed run the user can retry") is
*the* invariant — that's the choice a refactor would reverse without
realizing the implication.

Test for which: if the "why" content is itself the thing a refactor
could subtly break, keep it. If it just explains motivation a reader
could infer, cut it.

Cases: [generate-opaque-id-panic](./cases/2026-05-15-generate-opaque-id-panic.md),
[atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md).

---

## P-NAME-THE-REFACTOR-TRAP — name the likely-bad refactor in the body

When the invariant exists *because* a plausible-but-misguided refactor
instinct would break it, name the refactor in the intent body.
"Sorting or hashing the result would silently break X." "Parallelism
would silently break Y." "Returning Result here would silently let
weak entropy through."

This is more useful than abstract invariants because it speaks the
language of the change a future reader is about to make. The agent
proposing the change sees their own proposal in the intent and stops.

Cases: [extract-source-order](./cases/2026-05-15-extract-source-order.md),
[walk-determinism](./cases/2026-05-15-walk-determinism.md),
[stamp-check-never-writes](./cases/2026-05-15-stamp-check-never-writes.md).

---

## P-AGENT-PROOFING — "intentional, not incomplete" when design stops short

Agents and new programmers default to "let me complete this" or "let
me make this consistent." When a design deliberately stops short of
what looks like the obvious next step (one cycle reported vs. all
cycles, no Result on a panic-on-failure function), say *intentional,
not incomplete* explicitly — the literal phrase, or one like it. Costs
three words; prevents an entire class of well-intentioned regressions.

Cases: [cycle-first-only](./cases/2026-05-15-cycle-first-only.md).

---

## P-VERIFY-MATCHES-SHAPE — verify level tracks the load-bearing claim's shape

Pick the `verify` level based on the *verifiability shape of the
load-bearing claim*, not the importance of the intent or the
testability of side claims.

| Load-bearing claim is… | `verify =` |
|---|---|
| Runtime property a mined assertion or test can catch | `"test"` |
| Design decision / refactor-trap / "intentional, not incomplete" — reviewable by reading code, not reducible to a runtime check | `"neural"` |
| Formal-proof candidate (algorithmic invariant amenable to a solver) | `"full"` |
| Pure coordination convention with no checkable shape | `false` |

Over-marking design-philosophy intents as `"test"` is dishonest — no
test will ever be derived, so it pollutes the verification pipeline
with permanently-unverifiable entries. The verifier reports
`status=unknown` forever and the user learns to ignore it.

Under-marking testable invariants as `"neural"` wastes the testing
pipeline's stronger signal.

P-WHY-AS-INVARIANT and P-VERIFY-MATCHES-SHAPE are coupled: any intent
whose body relies on "why" content to be load-bearing is probably a
`"neural"` intent, not a `"test"` intent.

Cases: [generate-opaque-id-panic](./cases/2026-05-15-generate-opaque-id-panic.md)
(verify shift test → neural), [atomic-write-tempfile](./cases/2026-05-15-atomic-write-tempfile.md)
(verify shift test → neural), [did-you-mean-threshold](./cases/2026-05-15-did-you-mean-threshold.md)
(verify shift test → neural).

---

## Round-1 retrospective (2026-05-15)

- 15 intents reviewed across slices 14–18.
- 12 kept (with rewrites), 3 deleted.
- 3 verify shifts (`test` → `neural`).
- Most common authoring miss: **mixing motivation prose with the
  invariant** (10 of 15 intents had filler that obscured the
  load-bearing claim).
- Most surprising delete: an intent that claimed Rust's exhaustive
  match would silently miss a case — the type system already
  enforces what was claimed, and the failure mode described doesn't
  exist in Rust (case
  [matches-filter-type-system](./cases/2026-05-15-matches-filter-type-system.md)).
- Net signal-to-noise of dogfood intents after this pass: substantially
  higher; the index becomes a useful eval set for the skill once
  loaded in milestone D.

Next reflection trigger: end of milestone D (task #37 ships the
loading infra; first authoring round under loaded PHILOSOPHY.md
generates a new case batch).
