---
name: aristo-critique
description: Agentic prose-review of aristo annotations. Reads `.aristo/critique-queue/pending/<id>.toml` task files (each is self-contained — embeds the focal annotation + sibling/parent texts; no source reads needed). Workers loop on `aristo critique --pop-next` to claim one task at a time, submit categorized findings via `aristo critique --submit-findings`, and exit when the queue drains. The SDK is the sole writer of `.aristo/critiques/<id>.critique` files.
sdk_version: {{SDK_VERSION}}
---

# Aristo critique orchestrator

When the user invokes this skill (typically by typing `/aristo-critique`, or asking to "critique the annotations" / "review the prose"), follow this orchestration exactly. The SDK has already enqueued tasks; **your job is to produce findings the SDK validates and writes, NOT to write `.critique` files directly.**

Critique findings are **advisory** — categorized prose-quality suggestions on an annotation's text. They never block merge; they never modify source. The whole point is opinionated feedback that the user opts into.

## Step 1 — check the queue

The SDK enqueued each annotation as a separate self-contained task file at `.aristo/critique-queue/pending/<id>.toml`. Each task embeds the focal annotation text PLUS sibling and parent annotation texts — workers do NOT need to read source.

```bash
ls .aristo/critique-queue/pending/ 2>/dev/null | wc -l
```

If empty (or directory doesn't exist), report "no pending critiques" and stop.

## Step 2 — continuous dispatch with looping workers (max N=4 in flight)

**Workers loop, unlike verify.** Critique tasks are shallow (annotation text only; no deep code reads); reusing context across siblings is actually *helpful* for vocabulary alignment and parent-shape findings. So workers don't need the one-shot-per-task discipline verify uses — they loop on `--pop-next` until the queue drains and return their accumulated results.

Spawn up to **N=4** background workers via `Agent(run_in_background=true, model="sonnet", ...)`. Use the **Sonnet model** — critique is shallow prose work; Opus is overkill and slower. As each worker completes (drained the queue), check `--queue-status`; if pending > 0 (a worker terminated early via a CLI error and left stragglers), spawn one replacement. If pending == 0 AND in-flight == 0, proceed to step 3.

```
in_flight = 0
loop:
    status = aristo critique --queue-status
    if status.pending == 0 and in_flight == 0:
        break
    while in_flight < 4 and status.pending > 0:
        spawn Agent(run_in_background=true, model="sonnet", prompt=worker_prompt)
        in_flight += 1
        status = aristo critique --queue-status
    # wait for any worker to return; on each completion:
    #   in_flight -= 1; record the batch of results; re-check queue
```

Each worker gets **Bash tools only** — no `Write`, no `Read`. The task body is self-contained; the worker has nothing legitimate to read from the filesystem. **Workers cannot create `.critique` files directly**; the SDK is the sole writer via `aristo critique --submit-findings`.

Use a prompt structured exactly like this:

```
You are an aristo critique WORKER. Loop on `aristo critique --pop-next`
to claim queued annotation-critique tasks, decide categorized findings
from the embedded context, submit via `aristo critique --submit-findings`,
and exit when the queue drains. The task body is SELF-CONTAINED — you do
NOT need to read source files (and you cannot; you have Bash only).

## Worker loop

while true; do
  task=$(aristo critique --pop-next)
  if [ -z "$task" ]; then
    break        # queue drained — exit cleanly
  fi
  # parse $task (TOML), decide findings, submit
done

The task body printed by `--pop-next` has this shape:

```toml
id = "balance_no_duplicate_cells"
text = "Balance never duplicates cells across rebalance operations."
verify = "neural"
file = "src/btree.rs"                 # for reporting only; do NOT read this file
site = "fn balance_non_root (line 142)"
text_hash = "sha256:..."
body_hash = "sha256:..."

[parent]
id = "balance_invariant"
text = "The B-tree balance operation preserves the no-duplicate invariant."

[[siblings]]
id = "g3_no_cell_aliasing"
text = "Cell array slots do not alias across pages."

[[siblings]]
id = "cumulative_counts_disjoint"
text = "Cumulative cell counts are disjoint between adjacent leaves."
```

The `text` field is the FOCAL annotation you are critiquing. The
optional `parent` and `siblings` provide cross-annotation context for
parent-shape findings and vocabulary alignment.

## Your task per loop iteration

Decide whether the focal `text` has improvement opportunities along
any of these dimensions:

- **rephrasing**: prose could be clearer (double-negation, passive
  voice, ambiguous antecedent). MUST include a `suggested_text` that
  proposes a specific rewrite.
- **parent-shape**: the focal annotation overclaims/underclaims
  relative to its parent, or has a vocabulary mismatch with siblings
  in a way that breaks the parent's decomposition.
- **vocabulary**: word choice inconsistent with siblings (e.g., "cells"
  vs "records"). Suggested_text optional.
- **scope**: claim is broader or narrower than the focal site warrants.
- **clarity**: vague, weasel-worded, or hedged prose that obscures
  what's actually being claimed.

Each dimension has a severity:
- **strong-suggest**: author should act unless they have a specific reason not to.
- **suggest**: author should consider acting; not urgent.
- **info**: informational only; no action recommended.

If the focal text is fine, return ZERO findings — an empty critique
is a valid outcome and means "I looked, nothing to add."

## JSON schema (what you submit)

```json
{
  "critique": {
    "critiqued_at_text_hash": "<text_hash from task verbatim>",
    "produced_at_body_hash": "<body_hash from task verbatim>",
    "produced_by": "aristo-critique@v0.0.7",
    "attempts": 1,
    "findings": [
      {
        "category": "rephrasing",                 // ONLY: rephrasing | parent-shape | vocabulary | scope | clarity
        "severity": "strong-suggest",             // ONLY: strong-suggest | suggest | info
        "rationale": "...",                       // REQUIRED, non-empty
        "suggested_text": "..."                   // REQUIRED for category=rephrasing; optional otherwise
      }
    ]
  }
}
```

Do NOT include `finding_count` or `highest_severity` — the SDK derives
those on accept.

## Hard rules (validator rejects on violation)

1. `category` MUST be exactly one of: `rephrasing`, `parent-shape`, `vocabulary`, `scope`, `clarity`. No other values.
2. `severity` MUST be exactly one of: `strong-suggest`, `suggest`, `info`. No other values.
3. Every finding MUST have a non-empty `rationale`. A finding without a rationale is noise.
4. Findings with `category = "rephrasing"` MUST include a non-empty `suggested_text`. If you can't propose specific replacement text, use `category = "clarity"` or `info` instead.
5. `critiqued_at_text_hash` and `produced_at_body_hash` MUST be copied verbatim from the task body — the SDK checks they match the current index (staleness anchor).
6. `attempts = 1` always (workers don't retry).

## Submit

```bash
aristo critique --submit-findings --id <id> --json '<JSON-FROM-ABOVE>'
```

Wrap JSON in SINGLE quotes (JSON only uses double quotes internally, so the wrap is safe).

On accept: stdout has `accepted: sha256:<hex>` — capture the hash.
On reject: stderr has structured errors. Fix and retry up to 2 more times (3 total). After the third failure, log the failure to your accumulating summary and continue to the next task.

## Return when the loop exits

When `--pop-next` returns empty, return your accumulating per-task summary. One block per task, separated by `===`:

```
accepted: <id>  sha256:<hex>  findings=<N>
---
<TOML content from .aristo/critiques/<id-with-colons-to-underscores>.critique, verbatim>
===
accepted: <id-2>  sha256:<hex>  findings=0
---
<TOML content>
===
rejected: <id-3>  <one-line summary>
---
<full stderr from final failed submit attempt>
```

For each accepted task, do ONE `cat .aristo/critiques/<id-with-colons-to-underscores>.critique` via Bash to capture the SDK's stamped TOML form (with derived fields).

If your worker drained the queue without processing anything:

```
worker exited: queue drained on entry
```
```

When the worker returns, parse each `===`-separated block. Aggregate across all workers for the final summary.

## Step 3 — call `aristo critique --apply-findings`

Run the SDK's apply step via `Bash`:

```bash
aristo critique --apply-findings
```

This re-validates every `.critique` file against the current index (catches text drift between submit and apply) and prints a per-id summary grouped by severity. v0 of slice 27 is summary-only — the index is not yet updated with `last_critiqued_at_text_hash` etc. (deferred to v1).

## Step 4 — summary report

Emit a short markdown summary:

```
## Critique results

| Outcome | Count |
|---|---|
| Critiqued (with findings) | N |
| Critiqued (clean — no findings) | N |
| Rejected | N |
| Total findings | N (strong-suggest: A, suggest: B, info: C) |
```

If there are findings the user might want to act on (anything that's not info), offer the interactive review in step 5. Otherwise stop with a one-line "ok: critique complete."

## Step 5 — interactive review (gated by user choice)

For findings with `severity` ≥ `suggest`, offer to walk through them via `AskUserQuestion`. Same pattern as the neural-verify skill's step 7:

### 5.1 Opening choice

```
Question: How would you like to review the N actionable findings?
Options:
- Walk through all findings           — go through each suggestion step by step
- Strong-suggest only                 — focus on the most important findings
- Skip review                         — I'll come back later
```

### 5.2 Per-finding action menu

For each finding rendered:

```
Question: <id> [<category>, <severity>]. What next?
Options:
- Apply the suggested rewrite         — edit source to use suggested_text (rephrasing only)
- Defer / next finding                — leave the suggestion in the .critique file
- Stop review                         — exit step 5
```

For non-rephrasing findings (vocabulary, parent-shape, scope, clarity), the action menu skips "apply the suggested rewrite" — these usually need human judgment to act on. Just offer next/stop.

On **Apply**: read the current annotation text from the source file (via Edit tool), construct the diff, confirm via a second `AskUserQuestion` showing the proposed change, then apply. Run `aristo stamp` after — the text change will flip status to Stale and the entry will re-pend for fresh verify.

## What this skill does NOT do

- It does NOT write `.aristo/critiques/<id>.critique` files directly. The SDK is the sole writer via `aristo critique --submit-findings`.
- It does NOT modify `.aristo/index.toml`. (In v0, even `--apply-findings` doesn't modify the index. v1 will add caching fields.)
- It does NOT auto-apply suggested rewrites. Every source edit requires explicit user confirmation via a second `AskUserQuestion`.
- It does NOT have a default "all annotations" sweep. `aristo critique` requires `--filter` per `docs/decisions/critique-and-pipeline-architecture.md` §D6.

## Anti-patterns

- ❌ Granting `Read` or `Write` to a spawned worker. Critique workers are Bash-only; the task body is self-contained and exploration would just burn tokens.
- ❌ Spawning Opus workers. Use Sonnet — critique is shallow prose work, Opus is overkill.
- ❌ Verify-style one-shot workers for critique. Critique tasks are shallow; the per-spawn overhead exceeds the per-task work. Loop until drained.
- ❌ Using `category = "rephrasing"` without a `suggested_text`. The validator rejects this — use `clarity` or `info` if you can't propose specific replacement text.
- ❌ Returning more than 3-4 findings per annotation. If the annotation needs that much critique, it probably needs a rewrite or split, not a long list of nits.
- ❌ Submitting findings of severity `info` only. They surface as low-priority noise; if you have nothing actionable to say, return an empty critique (zero findings is a valid result).
- ❌ **Editing source without explicit confirmation.** Every source edit (when applying a rewrite suggestion) requires a second `AskUserQuestion` showing the diff before it lands.
