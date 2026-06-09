---
name: aristo-help
description: Orientation for the Aristo skill suite. Summarizes each bundled skill (what it does + the benefit) and gives example end-to-end scenario flows — new-project setup, PR review, canon-bound deep verify, and failure → fix-or-waive. Start here if you're not sure which `/aristo-*` skill to reach for.
sdk_version: {{SDK_VERSION}}
---

# Aristo skill suite — what's here and when to use it

When the user invokes this skill (typically `/aristo-help`, or "what aristo skills are there?", "how do I use aristo?", "which skill do I run?"), give them this orientation. Aristo turns the *intent* behind code into checkable, verifiable claims; the skills below drive the loop. Each is a zero-argument slash command (NL/menu routing, not flags).

## The skills

- **`aristo-authoring`** — capture intent *as you code*. Annotate the non-obvious decisions (a chosen invariant, a refactor trap, a deliberate-not-incomplete choice) with `#[aristo::intent]` while the rationale is fresh. **Benefit:** the *why* is recorded at the load-bearing site, in the user's words, where it can be verified — not lost to a stale comment.

- **`aristo-instrumenting`** — apply the `aristo::instrument::*` macros (`Inspect` derive, `expose_pub` attribute, `yield_point!` function-like) to make private SUT state observable to verification harnesses. Counterpart to `aristo-authoring` for the mechanical layer (annotations are claims about behavior; instrumentation makes state checkable). **Benefit:** harness-side observability without copy-pasted macro code per consumer.

- **`aristo-verify`** — verify intents and review the results. Opens a scope × mode menu (changed·neural by default, offline; full/both run server proofs in the background, sign-in gated), dispatches local one-shot workers and/or a server session, then walks the verdicts card by card (subject-only failure + success cards; fix-or-waive). **Benefit:** every claim is checked against the code, and nothing it concludes lands until the user accepts it.

- **`aristo-critique`** — agentic prose-review of the annotations themselves (rephrasing, parent-shape, vocabulary, scope, clarity). Advisory only; never blocks, never edits source. **Benefit:** opinionated feedback that makes the claims sharper, opt-in.

- **`aristo-intent-suggestions`** — review **canon matches**: the server's suggested canonical wording for your annotations, plus the sibling invariants of the same proof objective. **Benefit:** bind your claims to a shared catalog and adopt the related invariants you didn't think to write — high proof reuse.

- **`aristo-authored-review`** — review the intents *you just authored* (distinct from canon matches): list what's unreviewed (new-this-session vs backlog), then either kick a background critique pass (Critique-first) or do a light per-intent pass (mark reviewed / edit / delete). **Benefit:** a natural-pause checkpoint so freshly-written claims actually get a second look.

- **`aristo-status`** — a friendly, read-only project readout: tier + score, verification rate, counts, review backlog, pending canon, and the engine's recommended next action. **Benefit:** see where you stand at a glance, with one nudge toward the highest-value next step.

- **`aristo-help`** — this overview.

## Example scenario flows

**New project → first verified claim.**
`aristo init` → write code + `/aristo-authoring` (annotate the why) → `/aristo-verify` (changed·neural) → `/aristo-authored-review` (look over what you wrote) → commit (the git hook keeps the index honest) → `/aristo-status` to see the tier tick up.
*Yields:* the intent behind the code is captured, checked, and reviewed before it ships.

**PR review (CI).** On a pull request, an automated pass critiques the annotations added/modified and authors any missing intents on the changed code, then comments back — comment-only by default, no silent source mutation.
*Yields:* intent coverage + prose quality enforced at review time, not after merge.

**Canon-bound deep verify.** Sign in (`aristo auth login`) → `/aristo-verify` (full / both — server proofs in the background) → `/aristo-intent-suggestions` to adopt the matched canonical wording + sibling invariants.
*Yields:* claims bound to the shared catalog with reusable, server-verified proofs.

**Failure → fix-or-waive.** `/aristo-verify` surfaces a ✗ refuted card (the claim, the tested call-path, the violating path in *your* code) → **Fix in code** (edit + re-verify) or **Waive** (`aristo verify --accept <id> --because <reason>` — a tracked known-gap that the strict ratchet flips back to red if it ever starts passing).
*Yields:* refutations are loud and actionable; accepted gaps are reasoned and can't silently rot.

## Notes

- Proactive nudges (annotate after N edits, review at a pause, verify backlog, score/tier changes) come from the unified nudge engine and are configurable via `[nudges] aggressiveness = off|low|medium|high` in `aristo.toml`.
- Everything here is **subject-only**: the skills surface the user's own code and claims, never any internal verification model.
- All skills are installed per-agent with `aristo install-skills`; check for updates with `aristo install-skills --check`.
