---
name: aristo-neural-verify
description: Verifies aristo intent annotations whose `verify = "neural"`. Reads `.aristo/pending-neural.toml`, spawns one subagent per entry to produce a structured proof, writes proofs to `.aristo/proofs/<id>.proof`, and invokes `aristo verify --apply-verdicts` so the SDK's mechanical validator gates the status change.
sdk_version: {{SDK_VERSION}}
---

# Aristo neural-verification orchestrator

When the user invokes this skill (typically by typing `/aristo-neural-verify`, but also when they ask to "verify the neural intents" or "run aristo verify"), follow this orchestration exactly. The Aristo SDK has already done the dispatch + validator work; **your job is to produce verdicts for the SDK to validate, NOT to update the index directly.**

## Step 1 — read the pending request

The SDK wrote `.aristo/pending-neural.toml` listing every annotation needing neural verification. It has the form:

```toml
schema_version = 1

[[pending]]
id = "balance_no_duplicate_cells"
text = "Balance never duplicates cells across rebalance operations."
file = "src/btree.rs"
site = "fn balance_non_root (line 142)"
text_hash = "sha256:..."
body_hash = "sha256:..."

[[pending]]
id = "..."
...
```

If the file doesn't exist OR `pending = []`, report "no pending neural verifications" and stop. Do not invent work.

If the file lists entries, proceed to step 2.

## Step 2 — read the current index

Before spawning any subagent, read `.aristo/index.toml` once and keep it loaded. Subagents will need to look up cited intent / assume ids in it — they **must not** make up ids or recall them from memory. The index is the single source of truth for which annotations exist.

Pass the full index content into every subagent prompt so it can grep through it without an extra file read.

## Step 3 — spawn ONE subagent per entry (parallel where possible)

For each `pending` entry, spawn a fresh `Agent` subagent (use the `general-purpose` subagent_type). The subagent runs in its own context window, isolating its judgment from the other entries' verdicts. Use a prompt structured exactly like this:

```
You are verifying ONE aristo intent annotation. Produce a verdict in
the schema below. Do NOT modify any source files. Do NOT call the
aristo CLI. Your output is a TOML document that the aristo SDK
validator will then check structurally.

## Intent under verification

id:              {{id}}
text:            """{{text}}"""
file:            {{file}}
site:            {{site}}
text_hash:       {{text_hash}}
body_hash:       {{body_hash}}
prior_attempts:  {{prior_attempts}}  # 0 on first try; carried over from any
                                     # previous rejected proof for this id.
                                     # Use as `attempts = prior_attempts + 1`.

## Current index (for grounding lookups)

The following is the full content of `.aristo/index.toml`. Any intent
or assume ground you cite MUST appear here with the exact id you use.

```toml
{{index_toml_content}}
```

## Your task

Read the source file `{{file}}`, focus on `{{site}}` and surrounding
code. Decide whether the intent's claim holds:

- **verified**: the claim holds. Provide an informal proof tree.
- **counterexample**: the claim does NOT hold. Provide a concrete
  triggering trace.
- **inconclusive**: you cannot determine whether the claim holds.
  Describe the gap and suggest at least one annotation the user could
  add (an `aristo::intent` or `aristo::assume`) that would close it.

## Schema

You MUST emit a single TOML document in this shape (and nothing else
in your output — no commentary outside the TOML, no markdown fences):

[verdict]
type = "verified"        # or "counterexample" or "inconclusive"
method = "neural"
produced_at_text_hash = "{{text_hash}}"  # copy verbatim from prompt
produced_at_body_hash = "{{body_hash}}"  # copy verbatim from prompt
produced_by = "aristo-neural-verifier@v0.0.5"
attempts = {{prior_attempts}} + 1  # use the integer literal — e.g., prior_attempts=2 → write `attempts = 3`
property_kind = "invariant"  # or postcondition | precondition | equivalence | safety | progress

# Then exactly ONE of [verified] / [counterexample] / [inconclusive]:

[verified.proof]
conclusion = "<plain-English restatement of the focal intent>"

[[verified.proof.steps]]
path = "0"               # tree address — root is "0"; subgoals are "0.0", "0.1", "0.0.1", ...
claim = "<the step's claim>"
relation_to_parent = "decomposes"   # decomposes | instantiates | restricts | composes | excludes-counterexample
grounds = [
  # At least one ground per step. Variants — DO NOT include hash fields;
  # the SDK validator computes them mechanically from your citations:
  { kind = "intent",   id = "<id-from-index>", relation = "instantiates", reason = "..." },
  { kind = "assume",   id = "<id-from-index>", relation = "excludes-counterexample", reason = "..." },
  { kind = "code",     file = "src/x.rs", lines = "10-25", reason = "..." },
  { kind = "prior-step", path = "0.0" },
  { kind = "composition", reason = "subgoals combine via AND" },
]
subgoal_paths = ["0.0", "0.1"]    # if this step decomposes
proposed_promotion = false        # set true if THIS step's claim is reusable
                                  #   beyond this proof and is a candidate
                                  #   for becoming a standalone aristo intent

## Hard rules — the SDK validator rejects on any violation

1. Every step MUST have ≥1 ground. No "trivial" / "obviously" / "clearly" filler.
2. Prefer citing existing annotations (intent/assume by id) over re-deriving
   from code. Reading the index above tells you which ids exist.
3. **Cited id discipline.** Any `intent` or `assume` ground id MUST appear
   verbatim in the index TOML above. Do NOT guess, recall from memory, or
   approximate. Search the index for the id; if it isn't there, the ground
   is invalid — drop it or pick a different one. The validator rejects with
   "cited id `X` not found in current index" on any miss.
4. If you need an unstated assumption, do NOT inline it as a discovered ground.
   Return `inconclusive` with that assumption as a `suggested_annotation`.
5. **DO NOT write hash fields.** Omit `code_text_hash` from code grounds
   and `at_text_hash` from intent/assume grounds entirely. The SDK validator
   computes both from your citations (file+lines, or id lookup) and stamps
   them into the proof on accept. Writing your own hash is at best a wasted
   guess; at worst, a wrong guess that the validator rejects as staleness.
6. `prior-step` grounds must reference an EARLIER step (smaller path
   string). No cycles.
7. Tree branching: keep ≤ 3 subgoals per node. If you need more, split
   into intermediate intents with `proposed_promotion = true`.
8. For counterexample: violated_step_path must point at a step in your
   trigger_steps tree. trigger_steps must demonstrate the violating
   state concretely, not vaguely "could happen".
9. For inconclusive: gap.unfilled_path must name the path of the step
   you couldn't discharge. gap.suggested_annotations MUST have ≥1 entry.
10. `attempts` MUST equal `prior_attempts + 1` from the prompt above (you're
    a single-shot subagent; the SDK tracks repair budget across re-spawns
    via the existing .proof file on disk). When `prior_attempts = 0` (first
    try), write `attempts = 1`. The SDK refuses to dispatch beyond the
    K-bounded budget; you don't have to check it yourself.

## Write the file yourself

Before returning, use the `Write` tool to save your TOML verdict to
`.aristo/proofs/<id>.proof` where `<id>` is the entry's id with `:`
replaced by `__`. The path you write is exactly that string — no other
directory, no rename, no suffix beyond `.proof`. The SDK has already
moved any prior verdict for this id to `<id>.proof.bak` so your write
does not destroy history.

After writing, return ONLY the TOML you wrote (the same text). No
commentary, no markdown fences, no leading/trailing whitespace beyond
what the TOML body needs. The orchestrator will verify that the file on
disk matches the text you returned; mismatch is treated as failure.
```

**Important: the subagent writes the proof file itself, then returns the text it wrote.** This removes a round-trip — you (the orchestrator) don't re-write what the subagent already wrote, but you DO keep the text in memory for the interactive review in step 7 (saves a Read).

When the subagent returns, capture its output (the TOML it wrote to disk).

## Step 4 — verify the subagent's write (no duplicate write)

The subagent already wrote `.aristo/proofs/<id>.proof` directly. Your job here is defensive:

1. Confirm the file exists at the expected path (`<id>` with `:` → `__`).
2. Read the file once; assert the on-disk content matches the text the subagent returned. Mismatch → the subagent lied or fat-fingered; flag this loudly in the report and skip step 5 for this entry.

Do NOT re-write the file with the returned text. The subagent's write is the source of truth on disk; the returned text is only your cache for step 7.

If the file is missing entirely: treat the subagent as having failed; this is a parse-style failure that will surface in the summary.

## Step 5 — call `aristo verify --apply-verdicts`

Run the SDK's apply step via `Bash`:

```bash
aristo verify --apply-verdicts
```

This invokes the mechanical validator on every `.proof` file in `.aristo/proofs/`. The SDK will:
- accept verdicts whose schema, citation resolution (file exists, lines in range, id in index), and tree structure all pass — flipping status accordingly AND stamping computed hashes into the saved proof file
- reject verdicts that fail any check — printing the failure list to stderr with the proof file path
- exit non-zero if any rejection or parse error

## Step 6 — summary report

Emit a short markdown summary immediately after `--apply-verdicts` returns. Keep this tight; it sets up the interactive review in step 7. Format:

```
## Neural verification results

| Verdict        | Count |
|----------------|-------|
| Verified       | N     |
| Counterexample | N     |
| Inconclusive   | N     |
| Rejected       | N     |

(plus a one-line "why" per rejection, if any)
```

If everything was rejected or there is no accepted content: stop here. There is nothing to walk through.

## Step 7 — interactive review (gated by user choice)

After the summary, offer an interactive walk-through via `AskUserQuestion`. This is where the skill earns its keep — the user gets to actually act on counterexamples and suggestions, not just see them.

### 7.1 Opening choice

Ask the user how they want to engage:

```
Question: How would you like to review the results?
Options:
- Walk through all proofs           — go through each verdict step by step
- Counterexamples only              — focus on what was refuted
- Inconclusive only                 — focus on suggestions you could accept
- Skip review                       — I'll come back later
```

If the user picks "Skip review", stop. The proofs are on disk; they can be reviewed any time via `aristo show <id>` or by reading `.aristo/proofs/<id>.proof`.

### 7.2 Per-proof walkthrough — render the proof in markdown

For each proof in the chosen subset, render it human-readable. Pull the conclusion + steps + grounds from the on-disk `.proof` file (post-apply hashes are stamped, so the file is the source of truth). Format:

```
### Proof: <annotation-id>   (<verdict-type>)

**Claim:** <annotation text>
**Site:** <file>:<site>

**Conclusion:** <proof.conclusion>

**Reasoning:**
1. <step path 0 claim>
   - <ground 1 summary>
   - <ground 2 summary>
   1.1. <step 0.0 claim>  (relation_to_parent)
        - <ground summary>
   ...
```

For counterexamples: render the violation description + the trigger steps similarly.
For inconclusive: render the partial proof (if any) + the gap description + each suggested annotation as a numbered list.

Keep ground summaries short. For code grounds: `crates/x/y.rs:LO-HI — <reason truncated to 80 chars>`. For intent/assume grounds: `intent → <cited-id> — <reason truncated>`.

### 7.3 Per-proof action menu

After rendering each proof, ask the user what to do next. The options depend on verdict type:

#### Verified verdicts

```
Question: Verified — <annotation-id>. What next?
Options:
- Next proof                — continue
- Ask a follow-up question  — spawn a Q&A subagent with the proof loaded
- Stop review               — exit step 7
```

If the user asks a follow-up: spawn a fresh `Agent(general-purpose)` with the proof file + the relevant source loaded; relay the user's question; return the answer; loop back to the action menu.

#### Counterexample verdicts

```
Question: Counterexample — <annotation-id>. The proof shows the claim does NOT hold. What next?
Options:
- Fix the code            — make a source edit so the claim holds (you propose, user confirms)
- Rewrite the intent text — narrow the claim to exclude the failing case
- Defer                   — leave as Counterexample (will surface loudly on every aristo stamp)
- Next proof              — move on, decide later
```

On **Fix the code**: read the violated step, propose a specific edit (Edit tool — show the diff first via the question's `preview` field if reasonable). Confirm via a second `AskUserQuestion` before applying. After applying: run `aristo stamp` so the entry transitions to Stale, then re-pend for re-verification.

On **Rewrite the intent text**: read the current intent text from the source file, propose a narrowed version that excludes the failing case, confirm via `AskUserQuestion` with the new text in `preview`. Apply via Edit. Run `aristo stamp`. The entry transitions to Stale, will be re-verified on next `/aristo-neural-verify`.

On **Defer**: do nothing. Continue to next proof. The loud-counterexample warning on `aristo stamp` ensures it stays visible.

#### Inconclusive verdicts

This is the case the user specifically called out. **Every suggested annotation gets surfaced as an actionable question.**

```
Question: Inconclusive — <annotation-id>. The verifier suggests <N> annotation(s) that could close the gap. Pick one:
Options:
- Add suggestion 1: <kind> at <site> — "<truncated text>..."
- Add suggestion 2: <kind> at <site> — "<truncated text>..."   (if exists)
- Add suggestion 3: <kind> at <site> — "<truncated text>..."   (if exists; AskUserQuestion supports max 4)
- Skip — review later                  — leave the suggestions in the proof file
```

Use `preview` on each option to show the full suggested text (multi-line, with the file/site context). The user picks one (or "Other" for a custom variant of the suggestion).

On **Add suggestion N**: edit the source file at the suggested `at_site` to insert the new `#[aristo::intent(...)]` or `#[aristo::assume(...)]` annotation with the suggested text. Show the proposed edit (file path + new lines) and confirm via a second `AskUserQuestion` before applying. After applying: run `aristo stamp` so the new annotation is indexed. The new annotation will appear in the next `aristo verify` pending list (it needs its own verification).

If a proof has more than 3 suggestions: present the first 3 as direct options + a 4th option "Show all suggestions" that re-prompts with the next batch.

On **Skip**: continue. The suggestions stay in the proof file. The validator's suggestion-vs-index check (see `validator_rejects_inconclusive_when_suggestion_is_in_index`) means that IF the user later adds an annotation matching one of the suggestions, the entry auto-re-pends.

### 7.4 Closing

When the user picks "Stop review" or the walkthrough reaches the end:

- Print a one-line closing summary: actions taken this session (e.g., "3 proofs reviewed; 1 suggestion accepted (added assume on `atomic_write`); 1 counterexample deferred").
- If any source edits were made AND `aristo verify` hasn't been re-run yet: remind the user that the affected entries are now Stale and will be re-verified on the next `aristo verify` / `/aristo-neural-verify` cycle.

## What this skill does NOT do

- It does NOT modify `.aristo/index.toml` directly. The SDK does that via `--apply-verdicts`.
- It does NOT auto-fix rejected verdicts. The user reviews first.
- It does NOT call any LLM-as-judge between the verdict and the validator. The validator is purely mechanical; that's the design.
- It does NOT compute hashes. The SDK does that mechanically on accept.
- It does NOT touch source code unprompted. Source edits only happen as a direct result of a user choice in step 7 (accept suggestion, fix code, rewrite intent) — and every such edit is confirmed via a second `AskUserQuestion` showing the diff before it lands.

## Anti-patterns

- ❌ Spawning subagents with the same context already polluted by earlier verdicts. Each subagent must be a fresh `Agent(...)` call.
- ❌ Re-writing the SDK's pending file. The SDK regenerates it on the next `aristo verify` run.
- ❌ Including `code_text_hash` or `at_text_hash` in any ground — the validator computes and stamps them. Any value you write will be ignored at best and rejected at worst.
- ❌ Citing an intent or assume id you didn't find verbatim in the loaded `.aristo/index.toml`. The validator will reject "cited id not found in current index" and waste your repair budget.
- ❌ Returning a verdict on behalf of the user ("this seems verified to me"). The subagent is the only authority; you (this skill) orchestrate, you do not judge.
- ❌ **Walking through every proof verbatim without offering choices.** The interactive review only earns its keep when the user can actually act. Dumping all proofs to the chat is just noise.
- ❌ **Editing source without explicit confirmation.** Even when the user picks "Add suggestion 1", the actual `Edit` call only happens after a second `AskUserQuestion` that shows the proposed change. Skipping that step turns a typo into a silent regression.
- ❌ **Bulk-accepting all suggestions in one go.** Each suggestion is its own decision. If the user wants to accept many, they navigate through; the skill doesn't offer a single "accept all" option (too easy to land junk).
