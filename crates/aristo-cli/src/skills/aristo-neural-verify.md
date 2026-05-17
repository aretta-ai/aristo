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

## Step 2 — spawn ONE subagent per entry (parallel where possible)

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
produced_at_text_hash = "{{text_hash}}"  # copy verbatim
produced_at_body_hash = "{{body_hash}}"  # copy verbatim
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
  # At least one ground per step. Variants:
  { kind = "intent",   id = "<id>", at_text_hash = "<that intent's text_hash>", relation = "instantiates" },
  { kind = "assume",   id = "<id>", at_text_hash = "<that assume's text_hash>", relation = "excludes-counterexample", reason = "..." },
  { kind = "code",     file = "src/x.rs", lines = "10-25", code_text_hash = "<hash of those lines>", reason = "..." },
  { kind = "prior-step", path = "0.0" },
  { kind = "composition", reason = "subgoals combine via AND" },
]
subgoal_paths = ["0.0", "0.1"]    # if this step decomposes
proposed_promotion = false        # set true if THIS step's claim is reusable
                                  #   beyond this proof and is a candidate
                                  #   for becoming a standalone aristo intent

## Hard rules — the SDK validator rejects on any violation

1. Every step MUST have ≥1 ground. No "trivial" / "obviously" / "clearly" filler.
2. Prefer citing existing annotations (intent/assume by id from .aristo/index.toml)
   over re-deriving from code.
3. If you need an unstated assumption, do NOT inline it as a discovered ground.
   Return `inconclusive` with that assumption as a `suggested_annotation`.
4. Every intent/assume ground MUST include `at_text_hash` matching the
   cited entry's current text_hash in .aristo/index.toml.
5. Every code ground MUST include `code_text_hash` — sha256 of the specified
   lines, in `sha256:<hex>` form. Compute with: `sed -n 'LO,HIp' <file> |
   sha256sum`. Wrap as `sha256:<the hash>`.
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

## Step 3 — write each verdict to disk

For each subagent's returned TOML, write it to `.aristo/proofs/<id>.proof` — where `<id>` is the entry's id with `:` replaced by `__` (matching the SDK's filename convention).

Use the `Write` tool. Do NOT use the SDK to write the file; the SDK only reads proofs back during `--apply-verdicts`.

## Step 4 — call `aristo verify --apply-verdicts`

Run the SDK's apply step via `Bash`:

```bash
aristo verify --apply-verdicts
```

This invokes the mechanical validator on every `.proof` file in `.aristo/proofs/`. The SDK will:
- accept verdicts whose schema, hash anchoring, ground resolution, and tree structure all pass — flipping status accordingly
- reject verdicts that fail any check — printing the failure list to stderr with the proof file path
- exit non-zero if any rejection or parse error

## Step 5 — report the outcome to the user

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

## Anti-patterns

- ❌ Spawning subagents with the same context already polluted by earlier verdicts. Each subagent must be a fresh `Agent(...)` call.
- ❌ Re-writing the SDK's pending file. The SDK regenerates it on the next `aristo verify` run.
- ❌ Skipping a hash anchor because "the values are obviously the same." The SDK rejects on missing or mismatched hashes.
- ❌ Returning a verdict on behalf of the user ("this seems verified to me"). The subagent is the only authority; you (this skill) orchestrate, you do not judge.
