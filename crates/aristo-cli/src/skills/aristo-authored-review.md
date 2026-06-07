---
name: aristo-authored-review
description: Review the intents you (or the agent) just authored. Lists unreviewed `#[aristo::intent]` annotations via `aristo review --json` — split into new-this-session vs backlog — then drives an `AskUserQuestion` flow to either kick a background critique pass (Critique-first) or do a light per-intent pass (Review now). Marks reviewed via `aristo review --mark <id>`; the reviewed state lives in `.aristo/nudge-state.toml` and a later edit re-opens an intent. Subject-only: every prompt is about the user's own annotations.
sdk_version: {{SDK_VERSION}}
---

# Aristo authored-intent review

When the user invokes this skill (typically by typing `/aristo-authored-review`, asking to
"review the intents we just wrote" / "review my annotations", or after the aristo nudge
engine surfaced a review backlog at a natural pause), follow this orchestration exactly.

This is the consume side of authoring: you wrote intents; now you decide which are *good*.
It is distinct from `aristo-intent-suggestions` (which reviews **canon matches** — the
server's suggested wording) — this skill reviews the **authored** intents themselves, before
or independent of any canon match. The reviewed/unreviewed axis is local: it lives in the
git-untracked `.aristo/nudge-state.toml` reviewed map. Marking an intent reviewed records
the version you approved; **editing the claim or its covered code later re-opens it** (the
approval was for a specific version, not the id forever).

**Subject-only rule (load-bearing):** every line you surface is about the *user's own*
annotations and code. Never reference the verification model, internal proof objects, or
anything the user can't see in their own source.

## Step 0 — check for an active review session (FIRST, BEFORE ANY WORK)

Run `aristo session active`. Three cases:

- **Empty stdout** → no active session; proceed.
- **A session whose `kind` is a review kind you opened earlier** (e.g. `critique-review`
  from a Critique-first pass that didn't reach exit) → resume that flow (jump to the
  critique skill's review step); do NOT start a fresh pass.
- **A session of a different kind** → REFUSE. Print: "an aristo `<kind>` session is active
  (id=<id>, subject=<subject>). Exit it first with `aristo session exit` (or
  `aristo session exit --defer-undecided` to park open items), then re-invoke
  `/aristo-authored-review`." Stop here.

This is Layer 3 of the three-layer session guard (Layer 1 = the SDK refusing mutating
commands while a session is active; Layer 2 = the per-turn hook reminder).

## Step 1 — list what awaits review

```bash
aristo review --json
```

The snapshot has `unreviewed_count`, `new_count`, `backlog_count`, `baseline_captured`, and
an `intents` array (each: `id`, `text`, `file`, `site`, `status`, `verify`, `is_new`).

- **`unreviewed_count == 0`** → tell the user everything authored is reviewed, and stop.
- Otherwise lead with the count, using the split when `baseline_captured` is true:
  *"X new this session · Y backlog await review."* When `baseline_captured` is false, just
  say *"X intents await review"* (no SessionStart baseline was captured, so the
  new-vs-backlog split is not claimed).

## Step 2 — offer the approach (one `AskUserQuestion`)

Pop a single `AskUserQuestion` with the high-level choice. Lead with the **recommended**
option:

- **Critique-first (recommended)** — kick a background `aristo critique` pass over the
  unreviewed intents, keep working, then review its prose findings. Best when there are
  several intents or you want a quality second opinion.
- **Review now** — a light, per-intent pass right here: mark reviewed / edit / delete.
- **Not now** — defer; the backlog persists and the nudge will resurface it later.

### Critique-first (background)

**Guard ordering (D11):** kick the critique pass **BEFORE** opening any review session — the
session guard blocks `aristo critique` (and `aristo stamp`) while a session is active.

```bash
aristo critique --filter id=<comma,separated,unreviewed,ids>
```

Then hand off to the **aristo-critique** skill to run the worker loop and review findings in
its `critique-review` session. Marking an intent reviewed here is implied by acting on (or
dismissing) its critique findings — once you've addressed an intent's findings, run
`aristo review --mark <id>` for it.

### Review now (light, per-intent)

Present the unreviewed intents as an `AskUserQuestion` batch. The picker shows **at most
4 questions per page**, so paginate larger sets (one page of ≤4, decide, then the next).
For each intent, put its `text` + `site` in the option preview so the user reads the actual
claim. Per intent, offer:

- **Looks good → mark reviewed** → `aristo review --mark <id>`
- **Edit the wording** → edit the `#[aristo::intent("…")]` text in source, then
  `aristo stamp` (the re-stamp's hash drift re-opens it; mark it reviewed again once happy).
- **Delete it** → remove the annotation from source, then `aristo stamp`.
- **Skip** → leave it unreviewed (it stays in the backlog).

You can mark several at once: `aristo review --mark id1,id2,id3`. The whole batch is
validated before anything is written — a wrong id marks nothing and errors, so re-check the
id and retry.

## Step 3 — confirm and close

After the pass, run `aristo review` once more to show the remaining backlog (or "all
reviewed ✅"). Never edit `.aristo/` state files directly — `aristo review --mark` is the
only write path for the reviewed axis; source edits go through `aristo stamp`.
