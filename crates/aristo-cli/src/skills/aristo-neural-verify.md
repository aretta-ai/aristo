---
name: aristo-neural-verify
description: Verifies aristo intent annotations whose `verify = "neural"`. Reads `.aristo/pending-neural.toml`, spawns one subagent per entry which submits a JSON-serialized verdict via `aristo verify --submit-verdict` (the SDK is the sole writer of `.proof` files; subagents have no Write-tool access), and invokes `aristo verify --apply-verdicts` so the SDK's mechanical validator gates the status change.
sdk_version: {{SDK_VERSION}}
---

# Aristo neural-verification orchestrator

When the user invokes this skill (typically by typing `/aristo-neural-verify`, but also when they ask to "verify the neural intents" or "run aristo verify"), follow this orchestration exactly. The Aristo SDK has already done the dispatch + validator work; **your job is to produce verdicts the SDK validates and writes, NOT to update the index or proofs directory directly.**

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

For each `pending` entry, spawn a fresh `Agent` subagent (use the `general-purpose` subagent_type) **with only `Bash` and `Read` tools allowed** — no `Write`. The subagent runs in its own context window, isolating its judgment from the other entries' verdicts. **Subagents cannot create `.proof` files directly**; the SDK is the sole writer via `aristo verify --submit-verdict`.

Use a prompt structured exactly like this:

```
You are verifying ONE aristo intent annotation. Construct a JSON-
serialized verdict and submit it to the Aristo SDK via the CLI; the
SDK will validate the schema, write the .proof file on accept, and
return a sha256 anchor. You do NOT write any files yourself. You do
NOT modify any source files.

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

The full content of `.aristo/index.toml` is at that path. Read it
ONCE before citing any intent or assume ground. Any intent or assume
`id` you cite MUST appear verbatim in that file.

## Your task

Read the source file `{{file}}`, focus on `{{site}}` and surrounding
code. Decide whether the intent's claim holds:

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
    "produced_at_text_hash": "{{text_hash}}",   // copy verbatim from prompt
    "produced_at_body_hash": "{{body_hash}}",   // copy verbatim from prompt
    "produced_by": "aristo-neural-verifier@v0.0.6",
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
safe):

aristo verify --submit-verdict --id {{id}} --json '<JSON-FROM-ABOVE>'

On success: stdout contains `accepted: sha256:<64-hex>`. The SDK has
written `.aristo/proofs/{{id_with_colons_to_underscores}}.proof` and
stamped any computed hashes. Capture the sha256 line.

On failure: stderr contains a structured `verdict rejected (N check(s)
failed):` block listing each failure with location + detail, OR a
`--json: parse error: ...` if the JSON itself is malformed. Read the
errors, FIX the JSON, and re-invoke the same command. You may retry up
to TWO times inline (so at most three total submit calls in one
subagent run). If still failing after the third try, give up and
return the last failure verbatim — the orchestrator will report it as
rejected.

## Return to the orchestrator

After a successful submit, do ONE Read of the on-disk proof file at
`.aristo/proofs/{{id_with_colons_to_underscores}}.proof` to capture
the SDK's TOML form (with computed hashes stamped). Return EXACTLY:

accepted: sha256:<the hex from submit stdout>
---
<the TOML content from the file, verbatim>

No commentary before or after. The two parts are separated by a single
line containing exactly `---`. The orchestrator computes
sha256(TOML-content) and compares with the reported hash for an
integrity check; mismatch is treated as failure.

If you exhausted retries without a successful submit, return EXACTLY:

rejected: <one-line summary>
---
<the full stderr output from the final failed submit attempt>
```

When the subagent returns, capture its output. The first line tells
you the outcome (`accepted:` vs `rejected:`); the body below `---` is
either the canonical TOML or the failure stderr.

## Step 4 — verify the acknowledgement

For each subagent's returned text:

1. Split on the first line containing exactly `---`. Header line first; body after.
2. If the header starts with `rejected:` — record as a rejection with the body as the failure detail; skip steps 4.3–4.5 for this entry.
3. If the header starts with `accepted: sha256:<hex>` — extract `<hex>`.
4. Compute the sha256 of the body (the canonical TOML). Many environments have `shasum -a 256` or `sha256sum`; an Aristo binary helper exists at `aristo show --json sha256 <stdin>` if needed. The simplest is `printf '%s' "$body" | shasum -a 256`.
5. Compare. Match → record as accepted; keep the body in memory for step 7 (avoids a re-Read). Mismatch → record as rejected with detail "subagent reported sha256 does not match returned TOML (subagent cache corrupted or fabricated); on-disk file is still the SDK's write, but the returned text cannot be trusted".

Do NOT write anything yourself. The SDK has already written the file.

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

- It does NOT write `.aristo/proofs/<id>.proof` files directly — neither this orchestrator nor the spawned subagents have Write-tool access to that directory. The SDK is the sole writer, via `aristo verify --submit-verdict`.
- It does NOT modify `.aristo/index.toml` directly. The SDK does that via `--apply-verdicts`.
- It does NOT auto-fix rejected verdicts beyond the subagent's two inline retries. The user reviews further failures first.
- It does NOT call any LLM-as-judge between the verdict and the validator. The validator is purely mechanical; that's the design.
- It does NOT compute ground hashes. The SDK does that mechanically on accept.
- It does NOT touch source code unprompted. Source edits only happen as a direct result of a user choice in step 7 (accept suggestion, fix code, rewrite intent) — and every such edit is confirmed via a second `AskUserQuestion` showing the diff before it lands.

## Anti-patterns

- ❌ Granting `Write` (or any other file-mutation tool) to a spawned subagent. Subagents are Bash + Read only; the submit-verdict CLI is the single write path.
- ❌ Spawning subagents with the same context already polluted by earlier verdicts. Each subagent must be a fresh `Agent(...)` call.
- ❌ Re-writing the SDK's pending file. The SDK regenerates it on the next `aristo verify` run.
- ❌ Including `code_text_hash` or `at_text_hash` in any ground — the validator computes and stamps them. Any value you write will be ignored at best and rejected at worst.
- ❌ Citing an intent or assume id you didn't find verbatim in the loaded `.aristo/index.toml`. The validator will reject "cited id not found in current index" and waste your repair budget.
- ❌ Using `relation_to_parent = "supports"` (or any value not in the exact five-variant list). Same for `"supports"` as a ground `relation`.
- ❌ Using `prior-step` to reference a CHILD path (e.g., step `"0"` citing `"0.0"` as prior-step). Prior-step is for siblings and uncles; parent→child wiring is `subgoal_paths` + `composition`.
- ❌ Citing code line ranges that exceed actual file length (`"1-200"` for a 40-line file). The validator computes file length and rejects out-of-range citations.
- ❌ Returning a verdict on behalf of the user ("this seems verified to me"). The subagent is the only authority; you (this skill) orchestrate, you do not judge.
- ❌ **Walking through every proof verbatim without offering choices.** The interactive review only earns its keep when the user can actually act. Dumping all proofs to the chat is just noise.
- ❌ **Editing source without explicit confirmation.** Even when the user picks "Add suggestion 1", the actual `Edit` call only happens after a second `AskUserQuestion` that shows the proposed change. Skipping that step turns a typo into a silent regression.
- ❌ **Bulk-accepting all suggestions in one go.** Each suggestion is its own decision. If the user wants to accept many, they navigate through; the skill doesn't offer a single "accept all" option (too easy to land junk).
