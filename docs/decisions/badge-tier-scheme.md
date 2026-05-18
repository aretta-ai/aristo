# Badge trust-tier scheme

**Status: PARTIALLY DECIDED 2026-05-18.** Tier names + hidden-tier mechanic
locked. Numeric score formula and tier cutoffs deferred to a follow-up
DECIDED block (see "Pending" section at the end). Code lands after the
formula is locked; targets a slice 31.5 follow-up to slice 31's offline
badge.

## Context

Slice 31 shipped `aristo badge` (commit `d917cd5`) with a single-color
flat SVG showing the total annotation count: `aristo │ ✓ N`. The
visibility-artifacts mockup
(`../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`)
described four `--metric` variants (`verified-count`, `verification-rate`,
`founding-member`, `design-partner`) that the v0 implementation deferred
to Phase 2.

Three problems surfaced before the v0 had even pushed:

1. **No quality signal.** A project with 74 unverified annotations gets
   the same green ✓ as a project with 74 verified-at-full ones. The
   badge looks like a trust signal but doesn't actually grade trust.
2. **No growth incentive.** Static badges (npm version, build:passing)
   underperform progress badges (Codecov coverage, Mozilla Observatory
   A–F) at sustained adoption. "✓ 74" doesn't reward a developer who
   raised verification rate from 0% to 30%.
3. **No upgrade funnel.** Free tier and paid tier produce visually
   identical badges. Paid features (server-bound `aristos:` annotations
   via `aristo sync`, `verify="full"` formal proofs) have no public-
   facing signal.

The risk of NOT addressing these: badge gets removed from READMEs
because it doesn't say anything useful, or worse, says the wrong thing.

The risk of OVER-addressing: Goodhart's law. Once the badge tier becomes
a target, projects pad their intent count with trivial getters, or game
the verification-rate by marking everything `verify=false`.

This document locks the tier-naming scheme and the hidden-tier mechanic.
The score formula and numeric cutoffs are explicitly deferred to a
follow-up discussion so the math can be calibrated against real
codebases before being committed to.

## Decisions

### D1. Five-tier scheme, four visible + one hidden

| Grade | Name | Visibility | Identity |
|---|---|---|---|
| D | **Aspirant** | visible | seeking the path; has annotations, minimal verification |
| C | **Apprentice** | visible | learning the practice; lint + critique passes |
| B | **Adept** | visible | demonstrating skill; meaningful verification coverage |
| A | **Ascendent** | visible (free-tier ceiling) | rising toward areté; near-full verification |
| A+ | **Areté** | **hidden** | excellence achieved; paid-tier formal proofs |

Rationale:

- **All five tiers begin with A.** Reinforces the brand etymology
  (Aretta ← Greek ἀρετή "excellence"; Aristo ← Greek ἄριστος "best").
- **Role-noun pattern, not state-adjective.** Aspirant / Apprentice /
  Adept / Ascendent are all roles on a path. State-adjectives (Annotated,
  Tested, Verified) break the rhythm and conflict with Aristo's
  vocabulary for `verify` levels.
- **Ascendent (the A tier) is non-terminal by design.** "Master" /
  "Expert" sound like endpoints; Ascendent explicitly says "rising"
  and leaves headroom. Without that headroom, the hidden A+ tier feels
  arbitrary; with it, A+ feels inevitable. The narrative arc reads
  in one breath: *"Every aspirant is one path away from areté."*
- **Aspirant for D, specifically.** Considered alternatives (Annotated,
  Attested, Acolyte, Adopter, Anchored). Aspirant uniquely combines
  "person-role" + "aiming at the capstone" + "neutral entry-point." Its
  classical/religious meaning ("someone seeking admission to an order")
  matches a developer who has added annotations but not yet done
  verification work.
- **Areté as the capstone, not Aristos.** Both are the brand's namesakes
  (Aristo SDK + Aretta company). Aristos = "best" implies ranking
  against others; Areté = "excellence" is intrinsic to the code itself.
  Aretta is the verb (bringing your code toward areté); reaching the
  Areté tier means the code has *arrived*. Fits a project-graded badge.

### D2. Spelling — Ascendent, not Ascendant

Both forms are valid English. **Ascendent** chosen over **Ascendant**
because:

- The Latinate -*ent* form (from *ascendere*) leans into the
  classical-roots aesthetic the rest of the scheme already commits to.
- The -*ant* form is more common in modern astrology / business
  contexts, which slightly muddies the connotation.
- All five tier words now share the A-prefix + a slightly archaic
  register (Aspirant / Apprentice / Adept / Ascendent / Areté).

If implementation surfaces a real reader-confusion problem, this can
revisit; the surface area is one string in one rendering function.

### D3. Hidden-tier mechanic for Areté

The Areté tier is **never displayed on free-tier projects**, regardless
of their visible-score. Reasoning:

1. **Free tier feels complete at Ascendent.** A free-tier project can
   reach the top *visible* tier without any "you're stuck at B, pay
   to unlock A" friction. No artificial paywall in the visible scale.
2. **Areté becomes a discovery moment, not a paywall.** Users see Areté
   on another project's badge → "what's that?" → docs → "ah, it's the
   paid-formal-proof tier." Pull, not push.
3. **Anti-gaming.** A hidden tier is impossible to fake. If you don't
   see Areté on your dashboard, you know exactly what's missing.
4. **Marketing voice writes itself.** *"Free tier reaches Ascendent.
   Areté is reserved for code formally proven via paid verification,
   server-stamped by Aretta. We named ourselves after it."*
5. **Apple / Stripe pattern.** Hidden-but-aspirational tiers
   consistently outperform visible-but-paywalled ones at conversion.

The Areté tier's existence is announced in the docs (where users
discover it through reading or by encountering it on other projects'
badges), NOT through CLI output on a free-tier project (no "you'd be
Areté if you upgraded" nudge — that's the wrong incentive shape).

### D4. Qualitative gate criteria for Areté

Areté requires **both** of the following, in addition to crossing the
visible-score ceiling:

1. **At least one `verify="full"` proof exists for the project**, with
   the proof in a `Status::Verified` state (clean, not `Stale` /
   `Counterexample` / `Inconclusive`).
2. **That proof is server-bound** (id in the `aristos:` namespace,
   indicating `aristo sync` has run and aretta.dev's server issued the
   binding certificate).

Combined: Areté requires a *paid* formal proof AND server-issued
certification. Both halves matter:

- Without (1): a project could pay for `aristo sync` to get
  `aristos:` ids without doing the actual paid formal verification.
  Wrong incentive — Areté shouldn't be a pure spend signal.
- Without (2): a project could run a local `verify="full"` proof
  (currently not available — `verify="full"` requires the paid server
  per the current roadmap; (24/25/26) deferred per
  `docs/deferred/verify-test-design.md`). Once those slices land,
  (2) is what distinguishes server-certified from self-asserted full
  verification.

Both checks happen in the badge command at metric-compute time. No
runtime server call needed (the index already carries the
`aristos:` prefix and the `verified_outcome` certificate for
server-bound entries).

### D5. Visible-score ceiling at Ascendent

The visible score formula (D7 — see below) is independent
of the Areté gate. A free-tier project can reach
`visible_score = 0.92` and STILL be **Ascendent**, because the Areté
gate (D4) is a separate hurdle. The visible-score scale therefore
*saturates* at Ascendent for any project that doesn't meet (D4).

Intentional shape:

- Gives free-tier a *clear ceiling* — there's a top to reach.
- Avoids the "you're at 0.85, you'd be Areté if..." pressure pattern.
- Areté is a discrete state change, not a continuous gradient.

### D6. Spelling-out tier definitions

Each tier name is a *role on the verification-mastery path*; the
score formula determines which role a project currently occupies.
The role-to-formula mapping is the formula's job (D-TBD); the
role names themselves are locked here.

Brief semantic gloss for each tier (binding on the formula):

- **Aspirant** — has annotations; little-to-no verification.
- **Apprentice** — has annotations + has passed lint / critique.
- **Adept** — meaningful share of intents in verified-clean status.
- **Ascendent** — high share of intents verified; near-full coverage;
  may include some `verify="full"` proofs but no server-bound ones
  (else they'd cross the D4 gate).
- **Areté** — meets D4 gate.

A project can move *up* (more annotations verified, paid features
unlocked) and *down* (annotations drift to `Status::Stale`, a proof
goes `Counterexample`). Tier is computed per-invocation from the
current index state.

### D7. Visible-score formula — depth × module-aware coverage

**Status: DECIDED 2026-05-18.** Locked in `harshest-yet-realistic`
posture per the calibration principle below.

```
contribution(intent):
    if intent.verify == false:                excluded entirely
    elif intent.status not in clean-verified: 0
    elif intent.verify == "full":             1.0
    elif intent.verify == "test":             0.8
    elif intent.verify == "neural":           0.6
    elif intent.verify == "true":             resolve via aristo.toml default

verifiable = { i : i.verify != false }
verification_ratio = Σ contribution(i) / |verifiable|     (0 if |verifiable|=0)

For each module m where fn_count(m) > 0:
    target(m) = max(1, ⌈√fn_count(m)⌉)
    intents_in(m) = count of verifiable intents at file == m
    local_credit(m) = min(1.0, intents_in(m) / target(m))

coverage_score = Σ local_credit(m) / |{m : fn_count(m) > 0}|

# Articulation floor — baseline credit for the act of writing intents
articulation_floor = min(|verifiable|, ART_CAP) × ART_PER_INTENT
                     where ART_CAP = 3, ART_PER_INTENT = 0.05
                     (floor saturates at 0.15)

visible_score = max(articulation_floor, verification_ratio × coverage_score) ∈ [0, 1]
```

Notes:
- `clean-verified` = `Status::Verified` ∪ `Status::Tested` ∪
  `Status::Neural`. All other statuses (Unknown, Stale,
  Counterexample, Orphan, Forged, PendingDeepen, Inconclusive)
  contribute 0 to the numerator.
- `fn_count(m)` counts `Item::Fn` + methods on `Item::Impl`,
  excluding `#[cfg(test)]`-gated items. Closures excluded.
  Macro-generated fns excluded. Trait default methods counted.
- Modules with zero functions (pure-type modules) are skipped
  entirely from the coverage denominator — they have no
  verifiable behavioral surface.
- Assumes are excluded from both numerator and denominator (per
  A5 — assumes are external invariants, not verification targets).
- `Status::Counterexample` contributes 0, same as Unknown. Not
  negative — that would incentivize deletion. The
  `aristo stamp` loud-warning surfaces counterexamples
  independently of the badge.
- **Articulation floor** addresses the cold-start case: a
  brand-new project that has articulated invariants (intents
  exist) but hasn't verified them yet still gets a small
  baseline. Caps at 3 intents × 0.05 = 0.15 — keeps the
  project in Apprentice range while they bootstrap their
  verification flow. Cannot reach Adept by articulation
  alone — verification is required for that. Anti-padding:
  beyond 3 intents, more articulation doesn't help the floor.
  The floor only "wins" the max when the verified score is
  below 0.15 (early-stage or heavily-unverified projects);
  for any meaningfully verified project, `ratio × coverage`
  exceeds 0.15 and the floor is irrelevant.

### D8. Tier cutoffs

| Tier | `visible_score` range |
|------|------------------------|
| Aspirant | `[0, 0.10)` |
| Apprentice | `[0.10, 0.35)` |
| Adept | `[0.35, 0.65)` |
| Ascendent | `[0.65, 1.0]` |
| Areté | (any score, with D4 gate met) |

Calibrated so that:
- A project with 30+ test-verified intents (verification_ratio = 0.8)
  spread evenly across all modules (coverage_score ≈ 1.0) reaches
  Ascendent at score 0.80. Free-tier `verify="test"` users (once
  slices 24/25 land) reach the top visible tier without paid features.
- Neural-only projects (verification_ratio ≤ 0.6) cap at Adept on the
  visible scale, regardless of how well-distributed. To pass Adept,
  free-tier users need actual assertion-based verification.
- Areté is a discrete state change via D4, not a continuous gradient.

### D9. Calibration posture — `harshest-yet-realistic`

The locked values above (target = √fn_count, sqrt scaling, cutoffs
at 0.10/0.35/0.65, no module-filter softening) are chosen to err
on the side of *underrating* projects. Specifically:

- The Aristo SDK itself, at the rebase point of this commit
  (89 intents, 41 modules with intents, 17 zero-intent modules
  with code), would compute `coverage_score ≈ 0.45` (estimated
  with proper test-fn exclusion). Under all-neural verification,
  visible_score ≈ 0.27 → **Apprentice**. Under all-test
  verification, visible_score ≈ 0.36 → **Adept** (lower edge).
- A "thoughtfully but partially annotated" library is expected
  to land at Apprentice or Adept under these settings.
- Reaching Ascendent requires substantial real verification
  coverage, not just lots of annotations.

This is deliberate. The principle: **easier to loosen later than
to tighten.** Tightening tiers retroactively (raising thresholds,
adding stricter filters) takes points away from users who already
hit a tier — UX disaster, mirrors airline mile devaluation. Loosening
(lower thresholds, exclude internal modules from denominator, soften
sqrt to log) hands out points users haven't gotten yet — frictionless.

Knobs explicitly preserved as **post-launch loosening candidates**
(in priority order, if real-world distribution warrants softening):

1. **Lower Ascendent cutoff** from 0.65 to 0.55 — most common
   adjustment if most projects cluster at Adept.
2. **Add `coverage_score` floor** — replace `verification_ratio ×
   coverage_score` with `verification_ratio × (0.3 + 0.7 ×
   coverage_score)` so a project with deep verification but limited
   spread isn't fully punished.
3. **Filter coverage denominator** — count only modules with ≥1
   public `pub` function (skip pure-internal helper modules).
4. **Switch sqrt → log** for `target(m)` — eases burden on
   very large modules (200+ fns: target drops from 14 to 8).

None of these can be done in the OPPOSITE direction post-launch.
Tightening side is locked here.

### D10. Worked example — Aristo SDK at rebase commit

Rough computation against this repo's state at `1b18cc0` (slice 27.7
in flight; 89 intents across 41 modules; 17 additional modules with
code but no intents):

| Scenario | `verification_ratio` | `coverage_score` (est.) | `ratio × cov` | `articulation_floor` | `visible_score` | Tier |
|----------|----------------------|--------------------------|----------------|-----------------------|------------------|------|
| All Unknown (no verify run) | 0.00 | 0.45 | 0.00 | 0.15 (capped) | **0.15** | Apprentice |
| All neural-verified | 0.60 | 0.45 | 0.27 | 0.15 | 0.27 | Apprentice |
| 50/50 neural+test mix | 0.70 | 0.45 | 0.32 | 0.15 | 0.32 | Apprentice |
| All test-verified | 0.80 | 0.45 | 0.36 | 0.15 | 0.36 | Adept (barely) |
| All full-verified + bound | 1.00 | 0.45 | 0.45 | 0.15 | 0.45 → Areté gate met → **Areté** |

The articulation floor lifts the unverified case from Aspirant to
Apprentice — modest but real recognition for having articulated
89 invariants. All other scenarios are unaffected: the verified
ratio exceeds 0.15 every time.

`coverage_score` of 0.45 reflects: 17 zero-intent modules drag the
average, and several large modules (`walk/extract.rs`, `index/
strings.rs`) have intent counts below their sqrt-target. To raise
coverage, the SDK needs more intents in under-annotated modules,
not deeper verification of existing ones.

This is the SDK's own dogfood pressure. The badge says: "you have
verified depth on what you've annotated, but your articulation
is sparse — keep expanding."

## Pending — to be decided in a follow-up document

The following are explicitly NOT locked by this document:

1. **Visual treatment** — exact hex colors per tier, optional glyphs
   (the Areté ✦ proposal), per-style variations for `for-the-badge`.

2. **Backward-compatibility surface** — does the `--metric=` flag
   eventually expose `count` / `rate` / `tier` choices? Slice 31 ships
   no `--metric` (count-only). The tier scheme defaults to `tier` once
   landed; `count` and `rate` remain accessible via the flag.

## Implementation hand-off

Once the pending items are locked:

- New slice (call it 31.5) on `session-B-docs` or a follow-up branch.
- Touches `crates/aristo-cli/src/commands/badge.rs` — add tier
  computation alongside the existing metrics, default the visible
  value to the tier name, expose `--metric={count,rate,tier}` to
  preserve the current-shipped behavior under `--metric=count`.
- Promotes a new scenario for the tiered output.
- Coordinates with `commands/show.rs::status_label` since both
  surfaces map index state to user-facing labels; might want a
  shared module.

The score formula will live in `aristo-core` (not `aristo-cli`) so
the same logic is reusable by future per-language SDKs (per K3 /
K5). Badge command consumes it.

## References

- Slice 31 implementation: commit `d917cd5` on `session-B-docs`.
- Original visibility-artifacts mockup:
  `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`.
- Etymology source: Aristotle's *Nicomachean Ethics* Book VI on the
  five intellectual virtues (areté as the unifying concept).
- Reference comparison points: Codecov tiers, Mozilla Observatory
  A-F grades, npm-audit advisory levels, shields.io style spec.
