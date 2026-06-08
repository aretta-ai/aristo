# Critique workers lack PHILOSOPHY.md context — deferred fix

**STATUS: deferred.** Surfaced 2026-06-08 during the first full `aristo critique`
pass over the SDK's own authored intents (118 intents, 177 findings). Captured
here so the gap isn't re-discovered the next time critique output reads
off-taste.

## The gap

`aristo critique` task files (`.aristo/critique-queue/pending/<id>.toml`) are
deliberately self-contained: each embeds the focal annotation text plus its
sibling/parent texts, and nothing else. Workers are told not to read source.

They are **also never shown `aristo-authoring-philosophy.md`** — the canonical
record of house taste (the P-principles). So workers critique against *generic*
prose-quality norms, not the project's own taste.

## Evidence from the 2026-06-08 pass

The dominant theme across all four workers (~majority of 177 findings) was
"strip the refactor-warning tail — state a pure positive postcondition; move
the rationale to a code comment." That directly contradicts the established
**`P-NAME-THE-REFACTOR-TRAP`** principle (name the plausible-but-misguided
refactor in the intent body so the agent about to make it stops). 147 of 177
findings were rejected on exactly this ground.

The genuinely valuable findings were the ones *orthogonal* to taste: a factual
error (PIPE_BUF misapplied to a regular-file write), a mis-named refactor trap
("hashing" doesn't reorder a Vec), and `P-SPEC-STYLE` nits (jargon, planning-
artifact references). Those survived review and were applied.

Net: a worker that could see the philosophy would have suppressed the bulk of
the noise itself, and the review would have been ~30 findings instead of 177.

## Options to fix (pick one when resumed)

1. **Embed the relevant P-principles in each critique task file.** The SDK
   already knows the focal annotation's `verify` level and shape; it could
   inline the 2–3 most-relevant principles (e.g. `P-NAME-THE-REFACTOR-TRAP`,
   `P-SPEC-STYLE`, `P-WHY-AS-INVARIANT`) so the task stays self-contained.
   Keeps the "workers don't read files" property. More plumbing in
   `crates/aristo-cli/src/commands/critique/pending.rs`.
2. **Load `aristo-authoring-philosophy.md` into the worker prompt.** Simpler —
   the critique skill `include_str!`s the philosophy (already shipped in-crate)
   into the worker instructions. Costs tokens per worker; the philosophy is
   ~10 KB. Bounded and shallow, so probably acceptable.

Recommendation in flight: option 2 for simplicity, unless per-task principle
selection (option 1) proves worth the plumbing once we see a second pass.

## Related

- `crates/aristo-cli/src/skills/aristo-authoring-philosophy.md` — the taste record.
- `CLAUDE.md` §10A — the skill-feedback loop this gap sits inside.
