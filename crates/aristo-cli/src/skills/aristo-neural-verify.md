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

id:        {{id}}
text:      """{{text}}"""
file:      {{file}}
site:      {{site}}
text_hash: {{text_hash}}
body_hash: {{body_hash}}

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
attempts = 1
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
10. attempts = 1 always (you're a single-shot subagent; you don't loop).

Output only the TOML. Nothing else.
```

When the subagent returns, capture its output. That's the proof body.

## Step 4 — write each verdict to disk

For each subagent's returned TOML, write it to `.aristo/proofs/<id>.proof` — where `<id>` is the entry's id with `:` replaced by `__` (matching the SDK's filename convention).

Use the `Write` tool. Do NOT use the SDK to write the file; the SDK only reads proofs back during `--apply-verdicts`.

## Step 5 — call `aristo verify --apply-verdicts`

Run the SDK's apply step via `Bash`:

```bash
aristo verify --apply-verdicts
```

This invokes the mechanical validator on every `.proof` file in `.aristo/proofs/`. The SDK will:
- accept verdicts whose schema, citation resolution (file exists, lines in range, id in index), and tree structure all pass — flipping status accordingly AND stamping computed hashes into the saved proof file
- reject verdicts that fail any check — printing the failure list to stderr with the proof file path
- exit non-zero if any rejection or parse error

## Step 6 — report the outcome to the user

Summarize:
- how many verdicts the SDK accepted (status flipped to `neural` for verified, `counterexample` for refuted)
- how many were rejected, with a one-line summary of WHY for each
- if any are inconclusive: list the suggested annotations the user could add

If the SDK rejected verdicts: do NOT immediately retry. The user reviews the rejections; they decide whether to ask you to repair. Repair is bounded — the SDK enforces `attempts ≤ 3`, so don't burn the budget mindlessly.

## What this skill does NOT do

- It does NOT modify `.aristo/index.toml` directly. The SDK does that via `--apply-verdicts`.
- It does NOT auto-fix rejected verdicts. The user reviews first.
- It does NOT touch source code. Verification is read-only on source.
- It does NOT call any LLM-as-judge between the verdict and the validator. The validator is purely mechanical; that's the design.
- It does NOT compute hashes. The SDK does that mechanically on accept.

## Anti-patterns

- ❌ Spawning subagents with the same context already polluted by earlier verdicts. Each subagent must be a fresh `Agent(...)` call.
- ❌ Re-writing the SDK's pending file. The SDK regenerates it on the next `aristo verify` run.
- ❌ Including `code_text_hash` or `at_text_hash` in any ground — the validator computes and stamps them. Any value you write will be ignored at best and rejected at worst.
- ❌ Citing an intent or assume id you didn't find verbatim in the loaded `.aristo/index.toml`. The validator will reject "cited id not found in current index" and waste your repair budget.
- ❌ Returning a verdict on behalf of the user ("this seems verified to me"). The subagent is the only authority; you (this skill) orchestrate, you do not judge.
