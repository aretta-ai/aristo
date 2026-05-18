# Critique pipeline + shared SDK↔skill infrastructure

**Status: DECIDED 2026-05-17.** Targets slice 27 (`aristo critique`) implementation
and a prep refactor of the existing verify pipeline. Code lands after this
document is committed.

This is the first entry under `docs/decisions/`. The pattern: ADR-style notes
for architectural commitments made *before* the code lands, distinct from
`docs/deferred/` (which captures work we explicitly paused). Decisions
recorded here are binding on implementation; deviations require revising the
doc, not silently drifting.

## Context

Slice 23 shipped `aristo verify` for `verify="neural"` annotations using a
CLI↔skill split: SDK writes `.aristo/pending-neural.toml`, the
`aristo-neural-verify` skill reads it, spawns one subagent per entry, each
subagent submits a JSON verdict via `aristo verify --submit-verdict --id X
--json '...'`, SDK validates and writes the `.proof` file, then `aristo verify
--apply-verdicts` updates the index.

Slice 27 introduces `aristo critique` — agentic prose review with categorized
findings (rephrasing, vocabulary, parent-shape, scope, clarity) and severity
tags (strong-suggest, suggest, info). Shape is structurally identical to
verify (request file → skill → subagent → submit → apply) but semantically
distinct.

Two questions surfaced before coding:

1. **Architecture: how much do verify and critique share?** Copy-paste, partial
   helpers, or full trait abstraction?
2. **Dispatch shape: is one-subagent-per-entry the right pattern for critique
   too?** Critique tasks are shallow; vocabulary alignment benefits from
   cross-context.

Research on GSD's framework (gsd-sdk + skill registry + .planning/ state tree)
confirmed our basic CLI↔skill shape is sound, but flagged three patterns we
should adopt: filesystem spot-check at handoffs, structured pause/resume, and
*queue-based dispatch with atomic claim*. The last one reshapes how we
dispatch workers and is the foundation for everything else below.

## Decisions

### D1. Queue-based dispatch via atomic filesystem rename

**Decision:** Per-entry queue files with directory-based state, not a single
`pending-X.toml` listing.

```
.aristo/<pipeline>-queue/
├── pending/        # entries waiting to be claimed
│   ├── <id-a>.toml
│   └── <id-b>.toml
├── claimed/        # entries assigned to a worker (subagent in flight)
│   └── <id-c>.toml
└── done/           # (optional, off by default) entries successfully submitted
```

Atomic claim = `rename pending/<id>.toml → claimed/<id>.toml`. POSIX guarantees
single-directory rename is atomic on a single filesystem; two workers racing
on the same file → only one rename succeeds; the loser sees ENOENT and pops
the next entry.

**Why:**
- Robust to worker crash: stale entries in `claimed/` are recoverable (reaper
  re-pends entries older than threshold).
- Trivial parallelism: spawn N workers, each calls `--pop-next` in a loop until
  drained. No orchestrator-side dispatch coordination.
- Natural backpressure: queue empty → workers exit cleanly.
- Better than the current `pending-neural.toml` single-file approach where
  assignments are fixed at orchestrator spawn time and a crashed worker
  silently drops its assignment.

**CLI surface** (applied to both verify and critique via shared
infrastructure):

| Command | Effect |
|---|---|
| `aristo <pipeline> --pop-next` | atomic claim of one pending entry; prints JSON of the task to stdout; exit 0 with empty output if queue drained |
| `aristo <pipeline> --submit-<artifact> --id X --json '...'` | validates payload; on accept writes the artifact + removes the claimed entry; prints `accepted: sha256:<hex>` |
| `aristo <pipeline> --queue-status` | prints counts in pending/claimed; for `--json` mode prints structured state |
| `aristo <pipeline> --reap-stale-claims [--age 30m]` | moves claimed entries older than `--age` back to pending (default 30m) |

Replaces the legacy `pending-neural.toml` once the verify refactor lands.
Migration: on first run after the refactor, if `pending-neural.toml` exists,
the SDK expands it into the new `.aristo/verify-queue/pending/` layout and
deletes the old file. Single-pass migration; no compat shim past v0.0.7.

### D2. Self-contained task payloads (per-pipeline contract)

**Decision:** Whether a queue entry embeds enough context for the worker to do
its job *without reading source* is a per-pipeline contract. Critique's
entries are self-contained; verify's are not.

**Critique queue entry** (self-contained):
```toml
# .aristo/critique-queue/pending/<id>.toml
[task]
id = "balance_no_duplicate_cells"
text = "Balance never duplicates cells..."
verify = "neural"                     # part of what may be critiqued
file = "src/x.rs"                     # for location reporting only
site = "fn balance (line 142)"        # for location reporting only
text_hash = "sha256:..."
body_hash = "sha256:..."

[[task.siblings]]
id = "g3_no_cell_aliasing"
text = "..."

[task.parent]
id = "balance_invariant"
text = "..."
```

The critique subagent gets **Bash only** (no Read, no Write). It pops, decides
findings from the embedded context, submits. Cannot wander into the repo.
Cannot wastefully read unrelated files.

**Verify queue entry** (NOT self-contained):
```toml
# .aristo/verify-queue/pending/<id>.toml
[task]
id = "balance_no_duplicate_cells"
text = "Balance never duplicates cells..."
file = "src/x.rs"
site = "fn balance (line 142)"
text_hash = "sha256:..."
body_hash = "sha256:..."
prior_attempts = 0
# verify worker MUST read source — no embedding makes sense
```

The verify subagent gets **Bash + Read** (no Write — SDK is sole writer of
`.proof` files). It pops, reads source files cited at `file:site`, traces
logic, constructs a proof tree, submits.

**Why per-pipeline:**
- Critique's work is bounded by the embedded text; embedding everything kills
  the "agent goes exploring" failure mode and slashes token spend.
- Verify's work fundamentally requires source-code traversal; we can't embed
  the entire codebase. The trade-off (worker can read anywhere) is unavoidable
  for that pipeline.

### D3. Per-pipeline worker tooling

**Decision:** The skill body restricts subagent tools per pipeline:

| Pipeline | Subagent tools | Rationale |
|---|---|---|
| Verify | Bash + Read | reads source; can't write |
| Critique | Bash only | self-contained queue entry; no exploration needed |

Tool restriction is enforced softly via prompt convention (Claude Code's
general-purpose subagent type exposes all tools; we tell the subagent
"do not use Write/Read"). Defense in depth: the SDK is the sole writer of
all artifact files, so even if a subagent ignored the rule and tried to write,
the artifact wouldn't be recognized as a verdict/critique.

### D4. Per-pipeline model selection

**Decision:**
- **Verify:** Opus 4.7 (deep code reasoning, errors compound across cited grounds)
- **Critique:** Sonnet 4.6 (shallow prose work; vocabulary alignment is the
  hardest part and well within Sonnet's range)

For v0, hardcoded in the skill body when spawning subagents:
```python
# In aristo-critique skill:
Agent(model="sonnet", subagent_type="general-purpose", ...)

# In aristo-neural-verify skill:
Agent(model="opus", subagent_type="general-purpose", ...)
```

For v1, expose as config:
```toml
# aristo.toml
[verify]
model = "opus"  # opus | sonnet | haiku

[critique]
model = "sonnet"
```

For v2, per-task budget overrides become possible (e.g., upgrade to Opus on
verify entries where `prior_attempts > 2`). Not designed yet; left as a
later optimization.

### D5. Filter syntax: comma-separated values

**Decision:** `--filter id=foo,bar,baz` accepts a comma-separated list of
ids (OR semantics within the value). Multiple `--filter` flags AND together
(existing semantics, unchanged).

Same grammar extension applies to `file=`, `parent=`, `status=`. Adopted
across verify, critique, list, show, lint — any command using the J2
filter grammar.

### D6. Default scope: filter-required, no implicit codebase sweep

**Decision:** `aristo critique` with no `--filter` errors with usage
guidance, listing the supported scope options. **No implicit `--all`
default.** Cost of an unbounded LLM sweep is too high to make accidental.

Initial supported scopes:
- `--filter id=X[,Y,Z,...]` — explicit ids
- `--filter file=path/to/x.rs` — annotations in that file

Future scopes (deferred):
- `--filter "file.rs:LO-HI"` — line-range form (J2 grammar extension)
- `--staged` — entries whose text changed since their last critique (uses
  `last_critiqued_at_text_hash` cache)
- `--all` — opt-in unbounded sweep
- `--since HEAD~3` — entries touched in last N commits

Caching via `last_critiqued_at_text_hash` (set on apply) skips entries
whose text hasn't drifted since their last critique — applies to all
scopes once landed.

Verify keeps its current default: process every annotation that's not in
terminal-clean state. The asymmetry is intentional: verify is the
load-bearing correctness loop (run it often); critique is advisory
(opt-in per concern).

### D7. Shared infrastructure (`pipeline/` module), per-pipeline driver code

**Decision:** Extract the workflow-shape primitives into
`crates/aristo-cli/src/pipeline/`; keep per-pipeline driver code separate.

```
crates/aristo-cli/src/
├── pipeline/
│   ├── mod.rs           # module surface
│   ├── queue.rs         # atomic pop/submit/reap; directory layout
│   ├── submit.rs        # JSON-parse + validator + atomic-write + sha256 return
│   ├── apply.rs         # scan output dir + re-validate + index update + prune queue
│   └── filter.rs        # J2 grammar with comma-list values (D5)
├── commands/
│   ├── verify/
│   │   ├── mod.rs       # driver: assemble queue entries, call pipeline::queue
│   │   ├── validator.rs # proof-tree integrity, ground resolution, hash anchoring
│   │   ├── eligibility.rs # which entries enter the verify queue (verify=neural after project default)
│   │   └── skill_body.md  # aristo-neural-verify skill manifest
│   └── critique/
│       ├── mod.rs       # driver: assemble self-contained queue entries
│       ├── validator.rs # finding-shape, category/severity enums
│       ├── findings.rs  # CritiqueReport struct + serde
│       └── skill_body.md  # aristo-critique skill manifest
```

No trait abstraction. Per-pipeline driver code calls shared utilities; each
pipeline's `mod.rs` orchestrates its concern. If after critique lands the
two driver files are 90%+ identical, then we promote to a trait. If they
have meaningful differences (which D2-D4 suggest they will), we leave them
as siblings.

### D8. Skill body shape

**Decision:** One bundled skill per pipeline, hand-written, mirroring GSD's
"skill-per-concern" pattern (no shared skill body template).

| Skill | Workers | Tooling |
|---|---|---|
| `aristo-neural-verify` | spawn N Opus subagents; each loops `--pop-next` until empty | Bash + Read per worker |
| `aristo-critique` | spawn N Sonnet subagents; each loops `--pop-next` until empty | Bash per worker |

Worker loop (same shape both pipelines):
```bash
while true; do
  task=$(aristo <pipeline> --pop-next)
  [ -z "$task" ] && break        # queue drained
  # ... do work, construct JSON ...
  aristo <pipeline> --submit-X --id "$id" --json "$json"
done
```

The orchestrator (the skill itself) spawns up to N workers (default N=4 for
critique, N=2 for verify since Opus calls are more expensive), waits for all,
then calls `--apply-X` to land the results in the index.

Parallelism cap is per-pipeline configurable:
```toml
[verify]
max_parallel_workers = 2

[critique]
max_parallel_workers = 4
```

## Implementation sequence

1. **Rename: review → critique.** Across `docs/ROADMAP.md` (slice 27 entry),
   the `docs/mockups/07-lint-review-skills/` directory (rename to
   `07-lint-critique-skills/`), the `Commands::Review` stub in `lib.rs`, and
   the `crates/aristo-cli/src/skills/mod.rs` test that mentions
   `aristo-review-skill`. Small, isolating change — single commit.

2. **Prep: extract `pipeline/` infrastructure.** Move pending-file handling
   into `pipeline/queue.rs` (new directory layout); extract the atomic
   submit gate to `pipeline/submit.rs`; extract the apply pass to
   `pipeline/apply.rs`. Refactor verify command to use the new infrastructure.
   Migration shim: detect old `pending-neural.toml` and convert to new layout
   on first run. Re-dogfood `aristo-neural-verify` to confirm zero regression.

3. **Slice 27: critique pipeline.** New `commands/critique/` module; new
   bundled `aristo-critique` skill; new validator for the FindingsFile schema;
   wire up `aristo critique` CLI surface (--filter, --pop-next,
   --submit-findings, --apply-findings, --queue-status). Includes the
   self-contained queue entry shape (D2) and the per-pipeline tooling (D3-D4).
   Dogfood on Aristo's own annotations.

4. **Follow-up: hardening.** Stale-claim reaper (D1), filesystem
   spot-check after `--apply-X` (GSD pattern), CritiqueReport apply updates
   `last_critiqued_at_text_hash` + `finding_count` in index.

5. **v1 follow-ups.** `--all`, `--staged`, `--since` for critique; line-range
   filter syntax; per-pipeline model config in `aristo.toml`;
   pause/resume handoff state for interactive step-7 review.

## CritiqueReport schema (locked for v0)

```toml
# .aristo/critiques/<id>.critique
[critique]
critiqued_at_text_hash = "sha256:..."   # text we reviewed (staleness anchor)
produced_at_body_hash  = "sha256:..."   # body when produced
produced_by = "aristo-critique@v0.0.7"
attempts = 1
finding_count = 2                        # derived; used by --apply for index cache
highest_severity = "strong-suggest"      # derived; used by `aristo status` summary

[[critique.findings]]
category = "rephrasing"            # rephrasing | parent-shape | vocabulary | scope | clarity
severity = "strong-suggest"        # strong-suggest | suggest | info
rationale = "Opens with double-negation; lead with positive property."
suggested_text = "For every B-tree balance operation, each cell ..."   # optional

[[critique.findings]]
category = "vocabulary"
severity = "info"
rationale = "Uses 'cells'; sibling annotations use 'records'."
# no suggested_text on info findings
```

Enums (lockstep with validator code):
- `category`: `rephrasing` | `parent-shape` | `vocabulary` | `scope` | `clarity`
- `severity`: `strong-suggest` | `suggest` | `info`

Additions to either enum require a validator change + a doc update here. Five
categories cover the failure modes seen in the mockup; we'll expand based on
what real critique runs surface, not speculatively.

## Index updates on apply

When `aristo critique --apply-findings` accepts a critique:

| Index field | Value | Purpose |
|---|---|---|
| `last_critiqued_at_text_hash` | text_hash from the critique | staleness check: if entry's current `text_hash` != this, entry has drifted → re-critique on next `--staged` |
| `last_critique_finding_count` | `finding_count` from the critique | `aristo status` summary line |
| `last_critique_highest_severity` | `highest_severity` from the critique | for filtering "show me entries with strong-suggest findings" |

Critique does NOT touch `status`. The index `status` field remains the
verify-pipeline's purview (Verified / Tested / Neural / Counterexample /
Inconclusive / Stale / Unknown / Orphan / Forged / PendingDeepen). Critique
adds metadata fields alongside, doesn't transition the status enum.

## Alternatives considered (and rejected)

- **One-subagent-per-entry for critique with no batching.** Rejected: even
  though shallow, fan-out overhead is meaningful for trivial work. Pop-queue
  with N workers gives the parallelism wins without dispatch ceremony.

- **Single inline orchestrator (no subagents) for critique.** Rejected:
  pollutes orchestrator context across N entries. Subagent isolation per
  task keeps the parent skill's context window predictable.

- **Full `ArtifactPipeline` trait abstraction.** Rejected for v0 per the
  rule-of-three: with only two pipelines we don't yet know which joints
  matter. Shared utilities in `pipeline/` capture the real reuse without
  locking down a trait shape we may revise. If verify and critique drivers
  end up 90%+ identical post-implementation, promote to a trait.

- **Single-file queue (`pending-X.toml`) with file lock for atomicity.**
  Rejected: directory-based queue with atomic rename is simpler (no lock
  library needed), more portable, and lets us inspect/manipulate individual
  entries with standard tools (ls, cat, mv).

- **Default `aristo critique` to `--all`.** Rejected: cost footgun. Explicit
  scope required.

- **Embedding source code into verify queue entries.** Rejected: source
  reads ARE the work for verify; embedding the file is just a stale snapshot
  the worker would still need to verify against current source.

## Open questions (do not block slice 27)

1. **Sibling embedding strategy for critique.** Which siblings get embedded
   in a task entry? All siblings under the same parent? Siblings in the same
   file? K nearest by ID similarity? Start with "all siblings under the same
   parent, up to N=5" and revisit after dogfood.

2. **Critique caching invalidation on parent text drift.** If a parent
   annotation's text changes, do children become stale for critique purposes?
   Probably yes (vocabulary alignment depends on parent), but adds an
   invalidation edge to the index. Defer until users hit this.

3. **Stale claim threshold for the reaper.** 30 minutes default is a guess.
   Real value depends on average verify/critique runtime per entry. Tune
   after first month of usage.

4. **Apply-time re-validation policy.** Verify re-validates at apply time
   (defends against drift between submit and apply). Critique COULD do the
   same (re-check that the focal annotation hasn't drifted), or skip it
   (critique findings are advisory; staleness will surface on next
   `--staged` run). Defer to first implementation: ship with re-validation
   for symmetry, downgrade if cost is measurable.

5. **Per-task budget override (D4 v2).** When/how to auto-upgrade model
   based on prior_attempts. Out of scope for slice 27.
