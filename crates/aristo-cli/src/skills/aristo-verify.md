---
name: aristo-verify
description: Drives the whole aristo verification loop. Opens with a scope × mode menu (Changed·neural is the default; full/both run server proofs and need sign-in), dispatches the neural arm via local one-shot workers (`.aristo/pending-neural.toml` → `aristo verify --pop-next` → `aristo verify --submit-verdict` → `aristo verify --apply-verdicts`; the SDK is the sole writer of `.proof` files) and the full arm as a background server session (`aristo verify --wait/--view/--tags`), then reviews results card-by-card (subject-only failure + success cards; server sessions may be summary-only until the server attaches per-annotation results — see step 2B) inside a proof-review session. Fix-or-waive via `aristo verify --accept --because`.
sdk_version: {{SDK_VERSION}}
---

# Aristo verification orchestrator

When the user invokes this skill (typically by typing `/aristo-verify`, but also when they ask to "verify the intents", "run aristo verify", "check the proofs", or "verify everything"), follow this orchestration exactly. The Aristo SDK does the dispatch + validator work; **your job is to drive the menus, produce verdicts the SDK validates and writes, and review results — NOT to update the index or proofs directory directly.**

This skill drives the **whole** verify loop: choose a scope and a mode, dispatch (neural locally and/or full on the server), then review the results. The cards you render are **SUBJECT-ONLY** — they describe the user's own code and claims, never any internal verification model.

## Step 0 — check for an active review session (FIRST, BEFORE ANY WORK)

Run `aristo session active`. Three cases:

- **Empty stdout** → no active session; proceed normally.
- **Stdout shows a session id with `kind: proof-review`** → there is a prior proof-review session in flight (e.g., from a previous turn that didn't reach exit). Resume it: jump to the results review (step 3) with that session id; do NOT enqueue new tasks or spawn workers, because the user has unfinished triage to handle first.
- **Stdout shows a session id with a different `kind` (e.g., `critique-review`)** → REFUSE. Print: "an aristo `<kind>` session is currently active (id=<id>, subject=<subject>). Exit it first with `aristo session exit` (or `aristo session exit --defer-undecided` to park open items), then re-invoke `/aristo-verify`." Stop here. Do not enqueue, spawn, or touch state.

This check is Layer 3 of the three-layer enforcement (Layer 1 is the SDK pre-check that refuses `aristo verify` while a session is active; Layer 2 is the `UserPromptSubmit` hook that reminds you every turn). The skill body discipline matters because some agents bypass the SDK check by going directly to a worker subagent — the body refusal in this step is the last line of defense.

## Step 1 — opening menu (scope × mode)

Pop ONE `AskUserQuestion` to choose what to verify and how. These are **recipes over the existing `aristo verify` flags** — no new grammar. Lead with the default:

```
Question: What would you like to verify?
Options:
- Changed · neural (default)  — re-verify only new/stale intents, locally (no sign-in needed)
- Changed · full / both       — server proofs for changed intents (runs in the background; needs sign-in)
- Everything (--rerun)        — force re-verify, including already-clean intents
- Filter…                     — pick by file / module / id
```

**Mode → flags mapping:**
- **Changed (incremental)** is `aristo verify`'s default — it skips clean-verified entries automatically. Nothing extra needed.
- **Everything** = `aristo verify --rerun` (force re-check of clean-verified entries too).
- **Filter…** = `aristo verify --filter <key=value>` — `id=`, `file=`, `parent=`, `status=`; repeatable, ANDed. Ask a follow-up for the filter clause(s).
- **neural** = the local one-shot worker flow (step 2A) — free, no sign-in.
- **full** = the server canon-verify session (step 2B) — paid; needs an `arta_*` token + the GitHub App + canon coverage.
- **both** = run the neural arm locally AND kick the full arm in the background.

**Mode gating (sign-in):** `full` / `both` require a live `arta_*` token. Check with `aristo auth status` (or proceed and let the SDK report). If not signed in, say so and offer: run `aristo auth login` first, or fall back to **neural** for now. Never block — neural always works offline.

## Step 2 — dispatch

### Step 2A — neural arm (local one-shot workers)

The SDK enqueues each annotation needing neural verification as a task file at `.aristo/verify-queue/pending/<id>.toml` (it also records pending neural requests in `.aristo/pending-neural.toml`). Workers atomically pop one task at a time via `aristo verify --pop-next`.

Quick queue check:

```bash
ls .aristo/verify-queue/pending/ 2>/dev/null | wc -l
```

If empty (or the directory doesn't exist) and no full arm is running, report "no pending neural verifications" and skip to step 3 only if a full session is in flight; otherwise stop. Do not invent work.

**Read the index once.** Before spawning any worker, load the index. `.aristo/index.toml` is a gitignored regenerable cache, so if it is absent run `aristo stamp` once to materialize it (outside any active session), then read it and keep it loaded. Workers look up cited intent / assume ids in it — they **must not** make up ids or recall them from memory. Pass the full index content into every worker prompt so it can grep without an extra file read.

**Continuous dispatch with one-shot workers (max N=5 in flight).** Each worker claims EXACTLY ONE task, processes it, submits, and exits — it does not loop. Verification is context-heavy (deep code reads, proof-tree construction, ground citations); reusing a worker would let one verification's context pollute the next, silently degrading proof quality. Dispatch is continuous, not wave-based: keep up to **N=5** background workers in flight; as soon as ANY finishes, spawn its replacement (if the queue still has work). Use Claude Code's `Agent(run_in_background=true)` so completions notify the orchestrator — no polling, no waiting on the slowest sibling.

```
in_flight = 0   # orchestrator-side counter; tracks live background workers

# 1. Initial fill: spawn workers until in_flight == 5 OR queue empty
loop:
    status = aristo verify --queue-status   # peek
    if in_flight >= 5: break
    if status.pending == 0: break
    spawn Agent(run_in_background=true, model="opus", prompt=worker_prompt)
    in_flight += 1

# 2. Continuous replenishment: on each completion notification,
#    record the result and (if more work) spawn one replacement
on_worker_complete(result):
    in_flight -= 1
    record result for the final summary
    status = aristo verify --queue-status
    if status.pending > 0:
        spawn Agent(run_in_background=true, model="opus", prompt=worker_prompt)
        in_flight += 1

# 3. Termination: when in_flight == 0 AND pending+claimed == 0 → done
```

Each worker gets **`Bash` and `Read` tools only** — no `Write`. **Workers cannot create `.proof` files directly**; the SDK is the sole writer via `aristo verify --submit-verdict`.

Use a worker prompt structured exactly like this:

```
You are a ONE-SHOT verify worker. Claim ONE task from the queue,
verify it, submit the verdict, and EXIT. Do NOT loop. The orchestrator
spawns a fresh worker for the next task — keeping your context clean
of unrelated verifications is the whole point.

## Step 1 — claim your task

Run exactly:

```bash
aristo verify --pop-next
```

The stdout is either empty (queue drained by a concurrent sibling
worker — exit cleanly without processing) or a TOML body in this shape:

```toml
id = "balance_no_duplicate_cells"
text = "Balance never duplicates cells..."
file = "src/btree.rs"
site = "fn balance_non_root (line 142)"
text_hash = "sha256:..."
body_hash = "sha256:..."
prior_attempts = 0   # may be omitted when zero
```

POSIX rename atomicity guarantees you and any concurrent siblings
will never receive the same task.

## Step 2 — read the index ONCE (for grounding lookups)

The index lives at `.aristo/index.toml`. It is a regenerable cache,
so if it is absent run `aristo stamp` once to materialize it, then
read it exactly once. Any intent or assume `id` you cite as a ground
MUST appear verbatim in that file.

## Step 3 — do the work

Read the source file `<file>` at the popped task's `file:site` and
decide whether the intent's claim holds:

- **verified**: the claim holds. Provide an informal proof tree.
- **counterexample**: the claim does NOT hold. Provide a concrete
  triggering trace.
- **inconclusive**: you cannot determine whether the claim holds.
  Describe the gap and suggest at least one annotation the user could
  add (an `aristo::intent` or `aristo::assume`) that would close it.

## JSON schema (what you submit)

Construct a single JSON object matching the ProofFile shape. Field
names are `snake_case`; enum variants are `kebab-case`. Top-level:

{
  "verdict": {
    "type": "verified",                         // or "counterexample" or "inconclusive"
    "method": "neural",
    "produced_at_text_hash": "<text_hash from popped task>",   // copy verbatim
    "produced_at_body_hash": "<body_hash from popped task>",   // copy verbatim
    "produced_by": "aristo-neural-verifier@v0.0.7",
    "attempts": <prior_attempts + 1>,           // integer literal — e.g. prior_attempts=2 → 3
    "property_kind": "invariant"                // or postcondition | precondition | equivalence | safety | progress
  },
  // Then exactly ONE of "verified" / "counterexample" / "inconclusive":
  "verified": {
    "proof": {
      "conclusion": "<plain-English restatement of the focal intent>",
      "steps": [
        {
          "path": "0",                          // tree address; root is "0"; children "0.0", "0.1", "0.0.1", ...
          "claim": "<the step's claim>",
          "relation_to_parent": "decomposes",   // ONLY: decomposes | instantiates | restricts | composes | excludes-counterexample
          "grounds": [
            { "kind": "intent",     "id": "<id-from-index>", "relation": "instantiates",            "reason": "..." },
            { "kind": "assume",     "id": "<id-from-index>", "relation": "excludes-counterexample", "reason": "..." },
            { "kind": "code",       "file": "src/x.rs", "lines": "10-25", "reason": "..." },
            { "kind": "prior-step", "path": "0.0" },
            { "kind": "composition", "reason": "subgoals combine via AND" }
          ],
          "subgoal_paths": ["0.0", "0.1"],      // children of this node (if any)
          "proposed_promotion": false
        }
      ]
    }
  }
}

Counterexample form: replace the `"verified"` block with
`"counterexample": { "violation": { "description": "...", "violated_step_path": "0.1", "trigger_steps": [ ... ] } }` —
same step shape inside trigger_steps.

Inconclusive form:
`"inconclusive": { "partial_proof": null_or_steps, "gap": { "description": "...", "unfilled_path": "0.1", "suggested_annotations": [ { "kind": "intent", "suggested_text": "...", "at_site": "fn foo (src/x.rs:42)", "rationale": "..." } ] } }`

## Hard rules — the SDK validator rejects on any violation

1. Every step MUST have ≥1 ground. No "trivial" / "obviously" / "clearly" filler.
2. Prefer citing existing annotations (intent/assume by id) over re-deriving
   from code. Reading the index file tells you which ids exist.
3. **Cited id discipline.** Any `intent` or `assume` ground id MUST appear
   verbatim in `.aristo/index.toml`. Do NOT guess, recall from memory, or
   approximate. Search the index for the id; if it isn't there, the ground
   is invalid — drop it or pick a different one. The validator rejects with
   "cited id `X` not found in current index" on any miss.
4. **Valid `relation_to_parent` values are EXACTLY these five — nothing else:**
   `decomposes`, `instantiates`, `restricts`, `composes`, `excludes-counterexample`.
   `supports` is NOT a valid variant. If you find yourself wanting a sixth, use
   `decomposes` (most general) and explain via grounds' `reason` fields.
5. **Valid ground `relation` values (for intent/assume grounds) are EXACTLY:**
   `instantiates`, `restricts`, `composes`, `excludes-counterexample`, `promoted`.
   No `supports`, no `decomposes` here either.
6. If you need an unstated assumption, do NOT inline it as a discovered ground.
   Return `inconclusive` with that assumption as a `suggested_annotation`.
7. **DO NOT write hash fields.** Omit `code_text_hash` from code grounds
   and `at_text_hash` from intent/assume grounds entirely. The SDK validator
   computes both from your citations and stamps them into the saved proof.
8. **`prior-step` references must point to an EARLIER step in the tree**
   (lexically SMALLER path string). A step at path `"0"` CANNOT cite
   `"0.0"` as a prior-step — `"0.0"` is a CHILD, not a predecessor.
   Use `composition` or `decomposes` to wire parent→child relationships;
   `prior-step` is for sibling-or-uncle dependencies (e.g., step `"0.2"`
   may cite `"0.0"` or `"0.1"` as prior-step).
9. Tree branching: keep ≤ 3 subgoals per node. If you need more, split
   into intermediate intents with `proposed_promotion = true`.
10. **Code-ground line ranges must be REAL.** Use small, focused ranges
    (function body, a specific match arm). Do NOT cite `"1-200"` or other
    overestimates — the validator computes the file length and rejects
    "line range `X-Y` exceeds file length Z". When in doubt, check the
    file length with `wc -l` first.
11. For counterexample: `violated_step_path` must point at a step in your
    `trigger_steps` tree. `trigger_steps` must demonstrate the violating
    state concretely, not vaguely "could happen".
12. For inconclusive: `gap.unfilled_path` must name the path of the step
    you couldn't discharge. `gap.suggested_annotations` MUST have ≥1 entry.
13. `attempts` MUST equal `prior_attempts + 1` from the prompt above
    (you're a single-shot subagent; the SDK tracks repair budget across
    re-spawns). When `prior_attempts = 0`, write `attempts = 1`.

## Submit the verdict

Invoke the SDK to validate and write the proof. Wrap the JSON in
SINGLE quotes (JSON only uses double quotes internally, so the wrap is
safe). The `<id>` is the `id` field from the popped task body:

aristo verify --submit-verdict --id <id> --json '<JSON-FROM-ABOVE>'

On success: stdout contains `accepted: sha256:<64-hex>`. The SDK has
written `.aristo/proofs/<id-with-colons-to-underscores>.proof`, stamped
the computed hashes, AND removed the task from the queue's `claimed/`
directory. Capture the sha256 line for the orchestrator report.

On failure: stderr contains a structured `verdict rejected (N check(s)
failed):` block listing each failure with location + detail, OR a
`--json: parse error: ...` if the JSON itself is malformed. Read the
errors, FIX the JSON, and re-invoke the same command. You may retry up
to TWO times inline (so at most three total submit calls per task). If
still failing after the third try, give up on this task and continue
with the next `--pop-next` — the failed task stays in `claimed/` and is
reaped (returned to `pending/`) by a later run; record the failure in
your accumulating per-task summary.

## Return to the orchestrator

You processed exactly one task (or none, if the queue drained before
you could claim). Return ONE block matching one of these shapes:

**On accept:** do ONE `Read` of `.aristo/proofs/<id-with-colons-to-underscores>.proof`
to capture the SDK's stamped TOML form, then return:

```
accepted: <id>  sha256:<hex>
---
<TOML content from the proof file, verbatim>
```

**On reject** (after exhausting up to 2 inline retries):

```
rejected: <id>  <one-line summary>
---
<full stderr from the final failed submit attempt>
```

**On empty pop** (sibling worker drained the queue before you ran):

```
worker exited: queue drained on entry
```

No commentary outside these formats.
```

**Handle each completion notification.** When `Agent(run_in_background=true)` notifies you that worker W completed:

1. Decrement `in_flight`.
2. Parse W's return text. Split on the first line containing exactly `---`. Header first; body after.
3. Header `worker exited: queue drained on entry` → no-op; fall through to the dispatch decision.
4. Header starts with `rejected:` → record as a rejection with the body as the failure detail; fall through.
5. Header starts with `accepted: <id>  sha256:<hex>` → extract `<id>` and `<hex>`, then run the integrity check: compute `sha256` of the body via `printf '%s' "$body" | shasum -a 256` and **compare with the reported hash**. Match → record as accepted; keep the body in memory for step 3 (avoids a re-Read). Mismatch → record as rejected with detail "worker reported sha256 does not match returned TOML".
6. After recording: `aristo verify --queue-status`. If `pending > 0`, spawn one replacement worker (`in_flight += 1`). Otherwise wait for the next completion.

Do NOT write anything yourself. The SDK has already written the file.

**Apply the verdicts.** Once the queue is drained and `in_flight == 0`:

```bash
aristo verify --apply-verdicts
```

This runs the mechanical validator on every `.proof` file in `.aristo/proofs/`, flips status on the ones that pass (stamping computed hashes), rejects the ones that fail (stderr + proof path), and exits non-zero if any rejection or parse error.

### Step 2B — full arm (background server session)

`full` / `both` dispatch a **server canon-verify session**. It is paid (needs an `arta_*` token, the GitHub App, and canon coverage) and runs in the **background** — kick it, then keep working; surface results when ready.

- Kick it **non-blocking** (do NOT pass `--wait` here): `aristo verify` (optionally `--tags <id1,id2>` to subset to specific canon-bound ids, or `--filter …` to scope). The SDK prints a session id and detaches.
- Continue with the rest of the prompt / the neural arm. When you want results, re-attach: `aristo verify --view <session-id>` (snapshot) or `aristo verify --view <session-id> --wait` (block until terminal, with a `still running…` heartbeat).
- **both** = run step 2A locally AND kick 2B in the background; review whichever completes (neural is usually first).

**Server-dependent detail level (current behavior).** Hosted-org servers currently return **summary-level** session results — status, verified/failed counts, and the exit code — with an empty `annotations` array; the per-annotation failure cards documented in step 3 are attached only when the server serves per-annotation results (they return at parity; the CLI already renders them whenever present). When a session response carries no annotations:

- Report the **summary + session id + dashboard link** as the server arm's outcome; that IS the result, not a failure of the skill.
- Skip the card-by-card walk for the server arm (there are no cards to render); the neural arm's local `.aristo/proofs/` review is unaffected.
- Ignore `waiver … matched no annotation this run` stderr notes from that run — an empty annotations array cannot prove a waiver stale, so do NOT advise removing waivers based on it.

Never block the user on the server session. If the token is missing, the SDK reports it — fall back to neural (step 2A) and tell the user.

## Step 3 — results review (card-driven, inside a proof-review session)

After the dispatched arms produce verdicts (neural via `--apply-verdicts`; full via `--view`), review them card by card. Wrap the whole review in a **proof-review session** so every decision lands as a substrate-recorded triage state. For the full arm, card-by-card review requires the session response to carry per-annotation results — a summary-only response (see step 2B) has nothing to card; report the tally and move on.

### 3.0 Open the session

If step 0 found no active session, start one now:

```bash
aristo session start proof-review --subject "<short description of what was verified>"
```

(If step 0 found a resumable proof-review session, you're continuing it — skip this.)

### 3.1 Tally header + opening choice

Lead with the tally, then ask how to engage:

```
Verify done — ✓ N verified · ✗ M refuted · ? K inconclusive

Question: How would you like to review the results?
Options:
- Failures first (default)  — walk the refuted + inconclusive items first (what needs action)
- Walk everything           — every verdict, step by step
- Successes only            — just the ✓ verified cards (a quick confidence pass)
- Inconclusive only         — focus on suggestions you could accept
- Skip (defer to backlog)   — `aristo session exit --defer-undecided` (un-triaged items go to the backlog, never silently dropped)
```

If the user picks a close option, run that command and stop. The proofs are on disk; reviewable any time via `aristo show <id>` or `.aristo/proofs/<id>.proof`.

### 3.2 RENDER-THEN-MENU (load-bearing)

For EVERY item: **render the card first** (markdown in the conversation, and/or carry it in the AskUserQuestion option `preview`), THEN pop the action menu. The user must SEE the card before choosing. Cards are **SUBJECT-ONLY** — about the user's code and claim, never any internal verification model.

#### Failure card (refuted — the DifferentialReport)

```
✗ Refuted — <id>        (<file>:<site>)
 Claim:  "<annotation text>"
 Tested: <call-path spine, e.g. f() → g() → h()>
 Found:  <the violating path, in the user's own code>
         ↳ <file>:<line>  <one-line of what goes wrong>
```

Pull the content from the SDK's DifferentialReport / x-ray spine for the refuted entry (the call-path spine + the violating path in the user's source). **No internal-model references.** Then the action menu:

```
Question: ✗ Refuted — <id>. What next?
Options:
- Fix in code   — address the refuted claim in source, then re-verify
- Waive         — `aristo verify --accept <id> --because "<reason>" [--tracking <ref>]` (records a known-gap in expectations.toml)
- Re-run        — re-verify just this intent
- Next / defer  — `aristo session decide --item <id>#verdict --bucket pending` (parks it; stays loud until resolved)
```

- **Fix in code:** read the violating path, propose a specific edit (show the diff in the question `preview`), **confirmed via a second `AskUserQuestion`** before applying. After applying, record `aristo session decide --item <id>#verdict --bucket accepted --note "fixed in source"`. A re-verify runs AFTER the session closes (the guard blocks it during the session) to refresh the proof; the index itself regenerates on read.
- **Waive:** ask for the reason (mandatory), then `aristo verify --accept <id> --because "<reason>"` (optionally `--tracking <issue-url>`). This is write-only; it records the gap in `.aristo/expectations.toml` (commit it). The strict ratchet flips it back to red if the property ever starts passing, so waivers can't rot. Then `aristo session decide --item <id>#verdict --bucket accepted --note "waived: <reason>"`.
- **Re-run:** `aristo verify --filter id=<id> --rerun` after the session closes.
- **Next / defer:** records `pending`; the refutation stays loud on every read (`aristo status`, the cards) until resolved.

#### Success card (verified)

```
✓ Verified — <id>        (<file>:<site>)
 Claim:  "<annotation text>"
 Held:   <brief subject-only "why it holds" — one or two lines from the proof conclusion>
```

Action menu:

```
Question: ✓ Verified — <id>. What next?
Options:
- Mark reviewed   — `aristo session decide --item <id>#verdict --bucket accepted` (clears it from the proof-review backlog)
- View proof      — render the full proof tree (subject-only); then back here
- Next            — `aristo session decide --item <id>#verdict --bucket pending` (leave for later)
```

On **View proof**: render the conclusion + steps + grounds from the on-disk `.proof` file (post-apply hashes are stamped, so the file is the source of truth), keeping ground summaries short (`crates/x/y.rs:LO-HI — <reason ≤80 chars>` for code; `intent → <cited-id> — <reason>` for intent/assume). Subject-only. Then return to the menu.

#### Inconclusive verdicts

**Every suggested annotation gets surfaced as an actionable question.** Each suggestion has its own item ref `<id>#<suggestion_index>`.

```
Question: ? Inconclusive — <id>. The verifier suggests <N> annotation(s) that could close the gap. Pick one:
Options:
- Add suggestion 1: <kind> at <site> — "<truncated text>…"
- Add suggestion 2: <kind> at <site> — "<truncated text>…"   (if exists)
- Add suggestion 3: <kind> at <site> — "<truncated text>…"   (if exists; AskUserQuestion shows max 4)
- Skip — defer all                    — `aristo session decide --item <id>#verdict --bucket pending`
```

Use `preview` on each option to show the full suggested text (multi-line, with file/site context). On **Add suggestion N**: edit the source at the suggested `at_site` to insert the `#[aristo::intent(...)]` / `#[aristo::assume(...)]`, show the proposed edit, and apply only after a **second `AskUserQuestion`** confirms. Then `aristo session decide --item <id>#<N> --bucket accepted --note "added"`, and record the OTHER suggestions on that proof as rejected (with notes) so the substrate fingerprint captures the per-suggestion judgment. If a proof has more than 3 suggestions, present the first 3 + a 4th "Show all" option that re-prompts with the next batch. The added annotation re-pends on the next `/aristo-verify` cycle (the index regenerates on read; no stamp needed).

### 3.3 Closing the session

When all items are decided OR the user stops early:

- **All decided** → `aristo session exit` (strict close).
- **Stopped early** → `aristo session exit --defer-undecided` (un-decided items go to the backlog; never silently dropped).

Print the closing summary (the SDK emits one). If any source edits were made, remind the user the affected entries are now Stale (the index reflects this automatically on the next read) and will re-pend on the next `aristo verify` / `/aristo-verify` cycle.

## What this skill does NOT do

- It does NOT write `.aristo/proofs/<id>.proof` files directly — neither this orchestrator nor the spawned subagents have Write-tool access there. The SDK is the sole writer, via `aristo verify --submit-verdict`.
- It does NOT modify `.aristo/index.toml` directly. The SDK does that via `--apply-verdicts` (neural) and the server session (full).
- It does NOT auto-fix rejected verdicts beyond the subagent's two inline retries. The user reviews further failures first.
- It does NOT call any LLM-as-judge between the verdict and the validator. The validator is purely mechanical; that's the design.
- It does NOT compute ground hashes. The SDK does that mechanically on accept.
- It does NOT touch source code unprompted. Source edits only happen as a direct result of a user choice in step 3 (fix code, accept suggestion) — and every such edit is confirmed via a second `AskUserQuestion` showing the diff before it lands.
- It does NOT block the user on the full (server) arm — that runs in the background.

## Anti-patterns

- ❌ Granting `Write` (or any file-mutation tool) to a spawned worker. Workers are Bash + Read only; `--submit-verdict` is the single write path.
- ❌ Spawning workers with context already polluted by earlier verdicts. Each worker is a fresh `Agent(...)` call.
- ❌ Including `code_text_hash` or `at_text_hash` in any ground — the validator computes and stamps them.
- ❌ Citing an intent/assume id not found verbatim in the loaded `.aristo/index.toml` — the validator rejects "cited id not found in current index".
- ❌ Using `relation_to_parent = "supports"` (or any value outside the exact five-variant list). Same for `"supports"` as a ground `relation`.
- ❌ Using `prior-step` to reference a CHILD path (e.g. step `"0"` citing `"0.0"`). Prior-step is for siblings/uncles; parent→child wiring is `subgoal_paths` + `composition`.
- ❌ Citing code line ranges that exceed actual file length (`"1-200"` for a 40-line file).
- ❌ Blocking on the full server arm with `--wait` at dispatch time. Kick it detached; re-attach with `--view` later.
- ❌ Rendering an action menu BEFORE the card. Always render the card first — the user decides on what they can see.
- ❌ Referencing the internal verification model in any card. Cards are SUBJECT-ONLY: the user's code, the user's claim, the violating path in the user's source.
- ❌ Editing source without explicit confirmation. Even on "Fix in code" / "Add suggestion 1", the `Edit` only happens after a second `AskUserQuestion` showing the change.
- ❌ Waiving without a reason. `--accept` requires `--because`; a reasonless waiver is how a baseline rots.
- ❌ Walking every proof verbatim without offering choices. The review earns its keep only when the user can act; dumping all proofs to the chat is noise.
