# Aristo

*Verified intent for code.*

## Why Aristo

Aristo is a bet on how software engineering should be done:
**verifiable intent, inline with code.**

The intuition isn't new. Bertrand Meyer's Design by Contract argued
forty years ago that programs should carry their preconditions,
postconditions, and invariants right in the source. The practice
didn't catch on — too expensive to maintain by hand, and the
machinery to verify the contracts was rare. So we settled for
adjacent practices: docstrings, tests, type systems, code reviews.
Each catches a slice. None preserve full intent.

The agentic era forces the question open. Agents edit code faster
than the implicit channels — author memory, code review, "the team
just knows" — can carry intent through. And the same agents that
outpaced those channels can now draft annotations, verify claims,
and flag drift at a pace that makes the practice affordable for
the first time.

Aristo is the bet that this combination is real, and that the time
to make verifiable intent standard practice is now. Bets can fail.
This is the one we're making.

## The agentic gap

Agents are good at writing code. They are bad — structurally bad —
at remembering what code is *supposed to do* across the sessions,
context resets, and multi-agent handoffs that define how this work
now happens. A function gets edited; the test still passes; an
assumption nobody wrote down just broke.

The standard mitigations are partial:

- **Context files drift.** CLAUDE.md, AGENTS.md, design docs, plan
  files — they go stale within days because agents edit code faster
  than humans update prose. The next session reads a stale spec and
  confidently makes things worse.
- **Claims aren't verifiable.** When an agent says "this works,"
  it's a sentence, not an artifact.
- **Refactors preserve shape, break semantics.** Tests pass after
  the refactor because they only checked what was checkable.
  Unstated invariants — the ones the original author held in their
  head — break silently.
- **No audit trail.** When something breaks in production, you can
  read the code, but you can't reconstruct what the agent was
  *supposed* to be guaranteeing.

And it all happens faster than review can catch up.

## What Aristo is

Above a function, you state in one line what it's supposed to do.
You pick how that claim should be verified: by inspection, by tests,
by formal proof, or by an AI critic. The SDK hashes the function
body alongside the claim.

Concretely:

- **The annotation.** A one-line natural-language claim above a
  function, attached as a `#[aristo::intent("…")]` attribute. The
  annotation is the load-bearing artifact — it says what the code
  is *for*.
- **The hash.** A token-stream hash of the function body, captured
  when the claim was last verified. When the body drifts, the hash
  drifts, and the annotation's verification status reverts to
  *unknown*.
- **The verify spectrum.** Four modes, increasing in rigor:
  `verify=false` (a claim only — useful documentation, no
  verification), `verify="neural"` (an AI critic reads the code
  against the claim), `verify="test"` (checked against your
  existing tests — in progress), `verify="full"` (the strongest
  mode: the best check available, from tests up to formal proofs;
  currently with design partners).
- **The badge.** A trust-tier scheme — Aspirant → Apprentice →
  Adept → Ascendent → Areté — that summarizes how thoroughly a
  codebase practices the discipline.

## The name

*Aretê* (ἀρετή) in classical Greek means excellence — specifically,
excellence as habituated practice. It's the virtue of the
craftsperson who keeps showing up. *Aristos* (ἄριστος) is the
superlative: the best, the finest.

The names are deliberate. Aristo isn't a one-time audit you run
before a release. It isn't a credential you earn. It's a practice
you build into the codebase, a function at a time, every commit.
The tiers are stages on that practice ladder. The top tier — Areté
— is intentionally hard to reach, and unflashy when you get there.

## What Aristo isn't

Naming the negative space, because the name space is crowded:

- **Not a substitute for judgment.** Aristo verifies the claims you
  write — not whether you wrote the right claims. The hard part —
  choosing good intents, knowing what's worth claiming — is still
  your job. Sloppy intents in, sloppy verification out. The
  thinking and the discipline are yours; the structure for doing
  them well is what Aristo provides.
- **Not a context file.** CLAUDE.md / AGENTS.md tell agents how to
  work in this repo. Aristo tells the codebase what each function
  is for. Different layer, different lifetime: context files live
  next to the code; annotations live *in* the code, hashed against
  it.
- **Not a CI gate.** Aristo doesn't refuse to merge code that lacks
  annotations. Coverage is your choice. The badge surfaces it; the
  SDK does not enforce it.
- **Not a test framework.** Aristo invokes your tests when you ask
  it to (`verify="test"`); it doesn't replace them. Your test suite
  stays the source of truth for execution-time behavior.
- **Not a typechecker.** Types catch a particular slice of
  invariants — shapes, nullability, ownership. Aristo catches the
  slice types can't talk about: what the function is *for*, what
  assumptions it relies on, what would make it wrong without making
  it fail to compile.

## Why open source

The Aristo SDK is MIT-licensed, and stays that way. The annotation
language, the lint rules, the verify orchestration, the index
format, the badge generator — every piece you use to write and
verify intent in your codebase — is in the open. You can read it,
fork it, run it offline, ship it inside your product without
asking. (Specifics in [GOVERNANCE.md](../GOVERNANCE.md) and
[LICENSE](../LICENSE).)

Aretta — the company behind Aristo — will run a deeper
machine-checking backend and hosted solutions for teams that need
bespoke tailoring and stronger guarantees. Nothing here changes.

## The practice

Most codebases start at Aspirant. That's the honest tier — zero
annotations, no claims, no verifications. There's nothing wrong
with starting there; it's where everyone starts.

The climb is small, repeated motions: a function you wrote today,
a claim above it, a verify mode picked for that claim's
seriousness. Tomorrow another function. Next week, two more. The
badge tier moves when the codebase moves. The point isn't to be
Areté. The point is the practice — and the practice is what holds
the codebase together across the sessions, agents, and engineers
who'll touch it after you.
