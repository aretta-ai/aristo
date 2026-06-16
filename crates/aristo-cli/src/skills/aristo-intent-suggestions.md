---
name: aristo-intent-suggestions
description: Orchestrates the §17 proof-tree review loop. A choice-based explorer that (1) initiates the canon match, (2) reviews pending PRIMARY matches (your own annotations), and (3) reviews dragged-in SUGGESTIONS (the sibling invariants of the same proof objective). Drives the user through `AskUserQuestion` menus, finds placement sites, confirms each write, and adopts via the existing `stamp → canon accept` pipeline. All state lands in an `intent-review` session; the SDK is the sole writer.
sdk_version: {{SDK_VERSION}}
---

# Aristo intent-suggestions orchestrator

When the user invokes this skill (typically by typing `/aristo-intent-suggestions`, or
asking to "review canon matches / suggestions", "adopt the related invariants", or "what's
in the suggestions backlog?"), follow this orchestration exactly.

This skill is a **choice-based explorer**: you drive the user through `AskUserQuestion` menus
rather than free-form prose. The structure below was playtested and approved (§6A). You never
write `.aristo/` state files directly — every decision goes through `aristo session decide` and
every adoption goes through `agent writes #[aristo::intent] → aristo stamp → aristo canon accept`.
The CLI is the single write path; the agent writes source, the SDK manages state.

The skill is the **orchestrator for the whole loop**, not just a suggestions consumer (D9):
1. **initiate** the canon match (run the stamp/critique match step),
2. review **pending PRIMARY matches** — your own annotations the match flagged (new + carried-over),
3. review **SUGGESTIONS** — the proof-objective cluster dragged in alongside each primary.

## Two channels, one session

- **PRIMARY match** = a pending match for an annotation *you already wrote*. **Accepting it
  REWRITES** your wording to the canonical text and binds. The item ref is
  `match:<annotation-id>#<canon-id>`.
- **SUGGESTION cluster** = the *rest of the proof* the matched entry belongs to — the
  objective (parent) plus its sibling leaf invariants. **Adopting one ADDS a new annotation**
  (pure addition; nothing of yours is replaced). Item refs are `cluster:<key>` for the parent
  and `sibling:<key>#<canon-id>` for each dragged-in sibling.

Both are reviewed inside a single **`intent-review`** session, gated by the single-active-session
guard.

## Step 0 — check for an active review session (FIRST, BEFORE ANY WORK)

Run `aristo session active`. Three cases:

- **Empty stdout** → no active session; proceed normally.
- **Stdout shows a session id with `kind: intent-review`** → a prior intent-review session is
  in flight (e.g., a previous turn that didn't reach exit). Resume it: jump to the ROOT menu
  (Step 3) with that session id; do NOT re-run the match — the user has unfinished triage first.
- **Stdout shows a session id with a different `kind` (e.g., `critique-review`, `proof-review`)**
  → REFUSE. Print: "an aristo `<kind>` session is currently active (id=<id>, subject=<subject>).
  Exit it first with `aristo session exit` (or `aristo session exit --defer-undecided` to park
  open items), then re-invoke `/aristo-intent-suggestions`." Stop here. Do not run the match,
  start a session, or touch state.

This check is Layer 3 of the three-layer enforcement (Layer 1 is the SDK pre-check that refuses
mutations while a session is active; Layer 2 is the `UserPromptSubmit` hook). Always run it first.

## Step 1 — route to a mode (NL or menu) — §6B

The skill is a **zero-argument slash command** (`/aristo-intent-suggestions`), exactly like
`/aristo-critique`. There is no `/skill --flag`. A **mode** is reached two ways: (a) the user
states intent when invoking and you route, or (b) the ROOT menu (Step 3) offers it. Each mode is
just a **recipe of existing CLI flags** — no new machinery, no new grammar.

| Mode | When the user says… | Recipe (existing primitives) |
|---|---|---|
| *(default)* full | (nothing special) | initiate match → Stage A → Stage B |
| **backlog** | "just the backlog", "carried-over only", "don't re-match" | `aristo stamp --skip-canon` (no match) → seed session from cached/backlog items only |
| **status** | "what's pending?", "just the counts" | `aristo canon suggestions --counts` → print the table, **no session, no writes**, done |
| **matches** | "only the matches", "review my own annotations" | run **Stage A only** (skip Stage B) |
| **suggestions** | "only the suggestions" | run **Stage B only** (skip Stage A) |
| **for `<file>`** | "for src/storage/wal.rs" | add `--filter file=<path>[:LO-HI]` across both stages |
| **cluster `<objective>`** | "only the WAL cluster" | add `--filter parent=<kanon:objective>` across both stages (free — leaves carry `parent=`) |

Modes are **orthogonal and compose** (e.g. "backlog suggestions for src/storage/wal.rs"
= `--skip-canon` + Stage B only + `--filter file=core/storage/wal.rs`). Reuse the **existing
`--filter` grammar verbatim** (`id=`, `file=[:LO-HI]`, `parent=`, `status=`) — it already exists
for `critique`/`canon`; wire it through, don't reinvent.

- **status** short-circuits before any session: print the counts table from
  `aristo canon suggestions --counts`, then stop.
- **backlog** sets `--skip-canon` so Step 2 is skipped; the session seeds purely from
  carried-over items, and the entry Q&A then only offers the "carried-over" bucket.
- **matches** / **suggestions** drop the other stage from the run.

*(Deferred fast-follow modes — `staged`, `new`, `triage`, `resume` — are not in v1. If the user
asks for one, say it's not yet available and offer the closest v1 mode.)*

## Step 2 — initiate the match (skipped in backlog/status modes)

Unless the user chose **status** (short-circuit) or **backlog** (`--skip-canon`), run the match
step so the cache + suggestions queue are fresh:

```bash
aristo stamp            # default; matches new/changed annotations, fills the cache + queue
aristo stamp --skip-canon   # backlog mode: NO match — review only what's already queued
```

`aristo stamp` (or `aristo critique`) runs `POST /canon/match`; primaries land in
`canon-matches.toml`, dragged-in clusters land in the `.aristo/canon-suggestions-queue/`
(after the client-side dedup: drop members already bound / pending / rejected, and collapse
clusters sharing one objective into one task).

## Step 3 — open the session and show the ROOT menu

Read the at-a-glance counts (these gate the entry Q&A — new vs carried-over, D9):

```bash
aristo canon suggestions --counts
# → {"matches":{"new":N,"pending":N},"suggestions":{"new":N,"pending":N}}
```

If both channels are empty, report "nothing to review" and stop (no session needed).

Otherwise start the session (skip if Step 0 found a resumable one):

```bash
aristo session start intent-review --subject "<short description of the scope being reviewed>"
```

Then present the ROOT menu. **Every menu shows new vs pending counts in the option sub-line —
never silently hide capped items.**

```
ROOT — "Aristo Intent Suggestions — what would you like to do next?"
  1. Review canon matches        · "X new · Y pending"   → STAGE A
  2. Review canon suggestions     · "X new · Y pending"   → STAGE B
  3. Exit — leave everything pending
```

### The batch / multi-Q convention (used by both stages)

Same-kind items (N pending matches, N siblings) are presented as a **navigable batch** — one
`AskUserQuestion` call with multiple questions, where the user arrows left/right between items
and decides each, then you act on the whole batch. The picker caps at **4 questions per page**,
so larger sets **paginate** (e.g. 5 siblings = a page of 4 + a page of 1); say "page 1 of 2".

- Each **option** is an action; its **description** is the gray sub-line (counts, target site,
  confidence).
- An option's **preview** (monospace) carries the rich context: the `aristo canon show` card for
  decisions, or the code snippet for placements. Putting alternate sites on sibling options lets
  the user **compare candidate sites side-by-side** by arrowing between options.
- **Reuse, don't reinvent:** the decision card is literally the output of `aristo canon show <id>`
  — no bespoke card layout.

### The snippet rule (locked) — Stage A labeled rewrite vs Stage B pure-+

- **Stage A accept = REWRITE** an annotation that already exists. Do **NOT** render this as a
  `−`/`+` diff. A diff prefixes every line of the user's current wording with `−`, so the whole
  block reads as "all your text is being deleted" — confusing, because accept *replaces* the
  wording, it doesn't remove it. Instead show a **labeled before/after** so it is obvious which
  line is the current wording and which is the canonical replacement:

  ```
  Your text:     <the annotation's current wording>
  Match (NEW):   <the canonical text your wording becomes on accept>
  ```

  **Color (preferred, if the renderer supports fenced syntax highlighting):** put the pair in a
  ` ```diff ` fence so the two lines are tinted differently. Leave `Your text:` unprefixed
  (neutral) and prefix `Match (NEW):` with `+` so it renders green (the new wording). Do **not**
  prefix the user's line with `−`: nothing is deleted, and a red block is the exact confusion this
  rule removes.

  ```diff
    Your text:    <your annotation's current wording>
  + Match (NEW):  <the canonical wording it will be rewritten to>
  ```

- **Stage B adopt = ADD** a new annotation. Same failure mode as Stage A applies: do **NOT** render
  the suggested intent as a `−`/`+` diff or any block where the text is prefixed with `−`. Nothing
  of the user's is touched, so a removal-style block is misleading. When you show a suggested
  sibling or objective for a decision, label it:

  ```
  Suggested intent (NEW):
    <the canonical text of the sibling / objective>
  ```

  When you show the **placement** (Phase 3), give enough context that the target scope is
  unambiguous: include the signature line of the item the annotation attaches to (the `fn` / `impl`
  / `struct` / etc. it sits directly above) **and** the first 1-2 lines inside that scope's body,
  so the user can see *what* they are annotating, not just a bare insertion point. Render this
  surrounding code as plain context and mark only the new annotation line as an addition. In a
  ` ```diff ` fence, prefix just that one line with `+` (renders green); leave the existing code
  unprefixed. Never prefix the existing code or the suggested text with `−`. Shape (placeholders):

  ```diff
  + #[aristo::intent(text = "<canonical text>", parent = "<kanon:objective>")]
    <signature line of the target fn / impl / struct>
        <first 1-2 lines of its body, as plain context>
  ```
- **The canonical text is fixed by canon.** "Edit text/site first" edits the *site* (or hands the
  site choice to the agent); it does **not** reword the canonical phrasing.

## Stage A — pending PRIMARY matches

Read the open primaries from the match cache:

```bash
aristo canon list      # the primary-match cache (your own annotations); NOT the suggestions queue
```

Present them as a **navigable batch** (≤4/page, paginate). For each match the menu offers:

```
per match:  Accept (rewrite + bind) · Reject (keep your wording) · Skip · Back
  preview on Accept = a labeled before/after (NOT a diff): "Your text:" = your current wording,
    "Match (NEW):" = the canonical text it becomes on accept (see the snippet rule above)
  preview on the card = aristo canon show <canon_id>
```

Record each decision against the session:

- **Accept** → `aristo session decide --item match:<annotation-id>#<canon-id> --bucket accepted`.
  The kind's `on_accept` rewrites the annotation to canonical + binds (reuse of `aristo canon
  accept`). The annotation already exists — accept rewrites it.
- **Reject** → `aristo session decide --item match:<annotation-id>#<canon-id> --bucket rejected
  [--note "..."]`. Fingerprints the `canon_id` so it's suppressed on future runs.
- **Skip / defer** → `aristo session decide --item match:<annotation-id>#<canon-id> --bucket
  pending` (parks it; surfaces in the next session's backlog).

## Stage B — SUGGESTIONS, per cluster (parent-first, D6)

List the queued clusters (honor any `--filter` from the chosen mode):

```bash
aristo canon suggestions                                   # all queued clusters
aristo canon suggestions --filter parent=<kanon:objective> # cluster <objective> mode
aristo canon suggestions <objective>                       # one cluster's detail
```

**⟳ For each cluster task**, present the cluster gate. The **parent (objective) is decided once
per cluster** — this is the gate at the top of each cluster's review, asked once (there is no
global "re-decide the parent" loop):

```
cluster gate:  Explore cluster · Reject cluster · Skip · Back
  preview on Explore        = the cluster tree (✓ asserted / ☐ to consider), via aristo canon show
  preview on Reject cluster = "DISCARD dragged-in only / KEEP your independent match" (D6)
```

- **Reject cluster** → `aristo session decide --item cluster:<key> --bucket rejected
  [--note "..."]`. This runs the **D6 cascade**: discard the cluster's DRAGGED-IN siblings only,
  but **KEEP any member that independently matched on its own** (already bound in the index, or
  pending/accepted in the cache). The seeding primary is never discarded. Discarded members are
  fingerprinted so they don't re-surface. Go to the next cluster.
- **Explore cluster** → `aristo session decide --item cluster:<key> --bucket accepted`, then ADOPT
  the objective (see ADOPT below; if the objective already exists in source, skip it) and proceed
  to the three-phase sibling flow.

### The three-phase sibling flow (for an explored cluster)

**PHASE 1 — batch-DECIDE siblings** (multi-Q, navigable, ≤4/page):

```
per sibling:  Adopt · Reject · Skip      preview = aristo canon show <canon_id> card
```

When you surface the suggested text for the decision, label it `Suggested intent (NEW):` (see the
snippet rule) — never a `−`/`+` removal block.

Record each: Adopt is buffered (don't write yet); Reject →
`aristo session decide --item sibling:<key>#<canon-id> --bucket rejected [--note "..."]`
(fingerprints the canon_id). Siblings matching a prior rejection are auto-suppressed (dedup ④,
shared rejection log) — surface them in a separate "auto-rejected" group, don't clutter the main
list.

**PHASE 2 — agent batch-FINDS sites** for every "Adopt." For each adopted entry, the SDK exposes
its `applies_to` + `canonical_text` + `description` (via `aristo canon show <canon_id>`).
**Site-finding is YOUR job** (the agent): locate the load-bearing site(s) where the invariant
should be asserted. Propose a HIGH/MEDIUM-confidence primary site plus alternates.

**Choose the site as a staleness boundary, not just a topical match.** The element you attach to is
the scope Aristo hashes for re-checking: when its body changes, the adopted intent is re-verified.
Pick the smallest element whose every change should force a re-check and whose surrounding changes
should not (use the statement form to tighten scope inside a larger function). Too broad re-queues
the proof on unrelated edits; too narrow lets real changes to the governing code pass a now-stale
proof silently. See the `aristo-authoring` skill's "Scope: the annotation site is the staleness
boundary" for the full rationale.

**PHASE 3 — batch-CONFIRM placements** (multi-Q, navigable, ≤4/page):

```
per adopt:  Confirm — write here · Try a different site · Edit text/site first · Skip
  preview on Confirm         = the target scope's signature line + first 1-2 body lines as plain
                               context, with ONLY the new annotation line marked as an addition (a
                               single `+`/green line); never a removal block, never a bare insertion point
  preview on Different site   = the next candidate site (compare by arrowing)
  each candidate carries a confidence (HIGH/MEDIUM) in its sub-line
```

### ADOPT(entry) — the shared write micro-flow (D4)

Used for the parent objective AND each confirmed sibling. **Confirm → write → stamp → accept:**

1. `agent finds a candidate site (applies_to + canonical_text + description)`
2. **AskUserQuestion: confirm this site?** — *no* → re-find another site and re-ask; *yes* →
3. `agent writes #[aristo::intent(text=<canonical_text>, parent="<kanon:objective>")]` at the site
   (the canonical text verbatim — do NOT reword it).
4. `aristo stamp` — re-matches; the new annotation becomes a pending primary.
5. `aristo canon accept` — binds it.
6. Record `aristo session decide --item sibling:<key>#<canon-id> --bucket accepted --note "adopted"`.

Adoption funnels back through the **existing `stamp → /canon/match → canon accept` pipeline** —
there is no new source-mutation command; the agent writes code, the CLI manages state (the same
seam as the `aristo-authoring` skill).

## EXIT — close the session

When every item is decided, or the user picks "Exit / Stop":

- **All decided** → `aristo session exit` (strict close; succeeds because every item has a bucket).
- **Stopped early** → `aristo session exit --defer-undecided` (open items move to the per-kind
  **backlog**; the next session surfaces them — never silently dropped).
- **Abort entirely** → `aristo session abort --yes` (destructive; discards every decision).

Run `aristo stamp` for any adopted/applied edits **after** the session closes — stamp is a
mutation and the session guard blocks it while a session is open.

## What this skill does NOT do

- It does NOT write `.aristo/` state files directly. Decisions go through `aristo session decide`;
  the suggestions queue and rejection log are SDK-owned.
- It does NOT mutate source on its own for an adoption — the agent writes the annotation, then
  the CLI's `stamp` + `canon accept` bind it.
- It does NOT reword canonical text. Adopt uses the canon's phrasing verbatim; "edit" edits the
  *site*, never the wording.
- It does NOT re-decide a cluster's parent in a loop. The parent is decided **once per cluster**.

## Anti-patterns

- ❌ Skipping Step 0's `aristo session active` check. Layer-3 enforcement is the last line of
  defense; some agents jump straight to a decision and bypass the SDK pre-check.
- ❌ Rendering a Stage A accept as a `−`/`+` diff. A diff prefixes every line of the user's wording
  with `−`, which reads as "all your text is being deleted" and is the confusion this rule removes.
  Stage A is a **labeled before/after** ("Your text:" vs "Match (NEW):").
- ❌ Rendering a Stage B suggestion (sibling/objective) as a `−`/`+` diff or an all-`−` block. The
  same removal-block confusion as Stage A applies: nothing of the user's is touched. Show the
  suggested text labeled `Suggested intent (NEW):`, and the placement as plain context with only
  the new annotation line marked `+` (green).
- ❌ Inventing a bespoke decision card. The card is `aristo canon show <id>` output, reused.
- ❌ Cramming more than 4 questions onto one page. The picker caps at 4 — paginate and say
  "page 1 of 2".
- ❌ Writing an adopted annotation without the **confirm** AskUserQuestion. Every source edit needs
  explicit per-site confirmation before it lands.
- ❌ Rejecting a cluster and silently dropping a sibling the user already asserted. Reject-parent
  discards **dragged-in only** (D6); the SDK keeps any independently-matched member — trust it.
- ❌ Re-running the match in **backlog** mode. Backlog means `--skip-canon`: review only what's
  already queued.
- ❌ Opening a session for **status** mode. Status is `aristo canon suggestions --counts` only —
  no session, no writes.
- ❌ Starting an intent-review session and walking away without `exit` / `exit --defer-undecided`
  / `abort`. Every other aristo mutation refuses until the session closes; always reach a close.
