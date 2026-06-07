---
name: aristo-status
description: Pretty-prints a friendly project verification readout. Runs the read-only commands — `aristo metrics --json` (counts, verification rate, tier, score), `aristo status` (full breakdown by kind / verify level / status, plus review backlog + canon/skill health), `aristo review --json` (intents awaiting review, new-this-session vs backlog), and `aristo nudge` (the engine's recommended next action) — and synthesizes them into one digest. Read-only; never mutates the workspace. Subject-only: it reports on the user's own annotations and code.
sdk_version: {{SDK_VERSION}}
---

# Aristo status

When the user invokes this skill (typically by typing `/aristo-status`, or asking "how's my coverage / tier?", "show my aristo status", "where do I stand?"), produce ONE friendly digest of the project's verification health. This skill is **read-only** — it runs reporting commands and pretty-prints. It never edits source, never stamps, never starts a session.

**Subject-only:** everything you surface is about the user's own annotations, code, and progress — never any internal verification model.

## Step 1 — gather (read-only commands)

Run these and keep the output. None of them mutate anything:

```bash
aristo metrics --json     # machine-readable: intents, assumes, verifiable, verified_clean, unverified, verification_rate, tier, visible_score
aristo status             # full human breakdown: by kind / verify level / status, tier + score, review backlog, canon health, skill health
aristo review --json      # intents awaiting review: unreviewed_count, new_count, backlog_count, baseline_captured
aristo nudge              # the nudge engine's "would-surface" readout — the recommended next action (do NOT invent a parallel one)
```

If `aristo metrics` reports there is no workspace (or the command errors with "no aristo workspace"), tell the user this directory isn't an Aristo project yet (suggest `aristo init`) and stop. Don't fabricate numbers.

## Step 2 — synthesize ONE digest

Pretty-print a single readout. Use the ACTUAL values from the commands — don't recompute or guess. Suggested shape:

```
Aristo status — <project dir>

Tier:     <tier> · score <visible_score>      (ladder: Aspirant → Apprentice → Adept → Ascendent → ✦ Areté)
Verified: <verified_clean>/<verifiable>  (<verification_rate as %>)   ·   <unverified> unverified
Authored: <intents> intents · <assumes> assumes
Review:   <unreviewed_count> awaiting   (<new_count> new this session · <backlog_count> backlog, when a baseline exists)
```

Then add, only when non-empty (pull from `aristo status`):
- the **by-status** breakdown (verified / tested / neural / stale / counterexample / inconclusive / …) — surface any ⚠️ states (counterexample, forged, orphan) prominently;
- the **review backlog** (deferred session items) and any **active review session**;
- **canon health** + pending canon matches/suggestions (only when signed in);
- **skill health** (installed / stale agent skills), if `aristo status` flags drift.

Keep it tight and scannable. Don't dump raw JSON at the user.

## Step 3 — recommend the next action (via the engine, not a parallel nudge)

End with the single recommended next step. **Take it from `aristo nudge`'s readout** — that is the unified nudge/progress engine (#9), and routing through it keeps every surface consistent (don't hardcode a competing recommendation here). Phrase it as an offer, e.g.:

- engine recommends review → "You have <N> intents awaiting review — want to run `/aristo-authored-review`?"
- engine recommends verify → "<N> intents are unverified — want to run `/aristo-verify`?"
- engine recommends canon → "There are pending canon matches — want `/aristo-intent-suggestions`?"
- a win (tier/score up) → congratulate, briefly.
- nothing fired / `aggressiveness = off` → just report; no nudge.

If `aristo nudge` surfaced nothing, don't manufacture an action — a clean board is a good result; say so.

## What this skill does NOT do

- It does NOT mutate anything — no `stamp`, no `verify`, no `session start`, no source edits. It is a pure readout.
- It does NOT recompute tier/score/rate itself — those come from `aristo metrics` / `aristo status` (one source of truth; no drift).
- It does NOT invent next-step nudges — the recommendation comes from `aristo nudge` (the #9 engine), so it can never disagree with the proactive nudges.
- It does NOT reference any internal verification model — subject-only.
