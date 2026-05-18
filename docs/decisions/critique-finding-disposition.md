# Critique finding disposition — closing the loop

**Status: PROPOSED 2026-05-18.** Extends the v0 critique pipeline shipped
in commit `c8b7d8b`. Not yet approved for implementation; the higher-level
question of whether this is the right shape for "closing the loop on
critique findings" is still open.

This is the second entry under `docs/decisions/`. Sibling of
`critique-and-pipeline-architecture.md`, which it directly references.

## Context

Slice 27 v0 ships `aristo critique` and its interactive review (step 5
of the `aristo-critique` skill body). The review walks the user through
each finding and offers per-finding action menus. But the menus today are
threadbare for non-`rephrasing` findings: only **Next finding** /
**Stop review**. For `clarity`, `scope`, `vocabulary`, and `parent-shape`
findings — which by design carry no `suggested_text` — the user has no
way to mark a finding as *considered and dismissed* versus *not yet
looked at*.

Two failure modes result:

1. **Zombie findings.** Every `aristo critique --apply-findings` run
   re-surfaces the same suggestions the user has already triaged. There
   is no "I saw this; not acting on it" state.
2. **No audit trail.** When a finding is dismissed, the reasoning isn't
   recorded anywhere — neither the SDK nor the user has a persistent
   record of why critique finding #2 was passed over.

The first dogfood run on `critique_queue_entries_are_self_contained`
(2026-05-18) surfaced both: two substantive `suggest`-severity findings
that the user reviewed and deferred, with no mechanism to record either
the consideration or the rationale.

## Decisions (proposed)

### D1. Per-finding `disposition` field

Add a `disposition` field to each `Finding`, default-absent. Three closed
states + the implicit open state:

| `disposition` value | Meaning |
|---|---|
| absent (default) | Open — not yet reviewed. |
| `accepted` | User agrees with the finding; will act on it (or already has). |
| `rejected` | User disagrees — finding is wrong, not actionable, or doesn't apply at this site. |
| `deferred` | User has seen the finding and explicitly parked it. Different from `absent` (which means "not yet looked at"). |

Plus two metadata fields on closure:

| Field | Meaning |
|---|---|
| `disposition_note` | Optional free-text reason. |
| `closed_at` | RFC 3339 timestamp; SDK stamps on close. |

Schema:

```toml
[[critique.findings]]
category = "clarity"
severity = "suggest"
rationale = "..."
# additions:
disposition = "rejected"
disposition_note = "Annotation is intentionally narrative; rationale belongs in the prose."
closed_at = "2026-05-18T12:30:00Z"
```

### D2. New CLI: `aristo critique --close-finding`

```
aristo critique --close-finding --id <annotation-id> --index <N> \
                --disposition <accepted|rejected|deferred> \
                [--note "<free text reason>"]
```

SDK behavior: load `.aristo/critiques/<id>.critique`, mutate
`critique.findings[N]` with the disposition + note + timestamp, atomic
write. The SDK remains the sole writer of `.critique` files; this is the
single mutation path (siblings: `--submit-findings` creates them; the
new `--close-finding` modifies them; `--apply-findings` reads them).

Idempotent: closing an already-closed finding overwrites cleanly. No
diff is needed — last-disposition-wins. A future v2 could add a
mini-history field if multi-disposition audit becomes valuable; v1 keeps
it single-state.

### D3. Apply-findings filters open by default

`aristo critique --apply-findings` summary changes:

- **Default**: shows only findings with `disposition` absent (open).
- **`--include-closed`**: shows all, with disposition labels.

This is what makes the loop *close*. Once you triage a finding, it stops
showing up on every apply.

### D4. Skill action menu (revised step 5.2)

Replace the current per-finding action menu with:

```
Question: <id> [<category>, <severity>]. What next?
Options:
- Accept              — record agreement; will act on it (note optional)
- Reject              — record disagreement (note optional)
- Defer               — explicit "decided not now" (note optional)
- Apply rewrite       — for category=rephrasing only; edit source with suggested_text,
                        auto-close as accepted with closed_via="edit"
- Skip / next         — true skip; no disposition recorded; finding stays open
- Stop review         — exit step 5
```

The note prompting uses `AskUserQuestion`'s `Other` option for free-text
input. Per-finding, optional. Users who want to triage fast can pick the
disposition without a note.

For `Apply rewrite`: after applying, the source `text_hash` will drift,
which on next `aristo critique` regenerates the whole `.critique` file
from scratch. Disposition history for the prior findings is lost
intentionally — the annotation changed, the prior review no longer
applies. (This matches D6's text-drift invalidation semantics from the
parent design doc.)

### D5. Text-drift invalidates dispositions

When the focal annotation's text drifts (text_hash changes), the
existing `.critique` file is stale and gets re-generated on the next
`aristo critique --filter id=X` run. Old dispositions are lost
intentionally — the prior review was about prior text. Re-running
critique on the new text starts with a fresh slate.

Codifying this means: nothing tries to migrate dispositions across
text-drift. The simplicity here is load-bearing — there's no
fuzzy-match-disposition-from-old-finding-to-new mechanism. If a finding
is dispositioned, the annotation changes, and the new critique surfaces
a similar finding, the user triages it again. (Usually the new finding
is meaningfully different anyway.)

## New intents to author alongside

- `critique_finding_disposition_closes_the_loop` — describes the four
  states and what "closed" means.
- `close_finding_is_only_mutation_path_for_dispositioned_critiques` —
  SDK is the sole writer of `.critique` files even for disposition
  updates; agents do not edit them directly.
- `apply_findings_filters_open_by_default` — the default summary excludes
  closed findings; `--include-closed` opts in to the full view.
- `text_drift_invalidates_critique_dispositions` — staleness invalidation
  is total, not partial.

## Implementation sequence

1. **`aristo-core` schema additions:** `Disposition` enum + optional
   fields on `Finding`. Round-trip test for closed-finding TOML.
2. **`commands/critique/close.rs`:** new module with `run_close_finding`.
   ~80 LOC; mirrors `submit.rs` shape (parse args, load file, mutate,
   atomic write).
3. **CLI wiring in `lib.rs`:** `--close-finding`, `--index`,
   `--disposition`, `--note` flags. Update dispatch.
4. **`apply.rs` filter:** default skip closed findings; add
   `--include-closed` flag.
5. **Skill body rewrite (step 5.2):** new action menu with Accept /
   Reject / Defer / Apply rewrite (rephrasing only) / Skip / Stop.
   Includes the note-prompting flow.
6. **Build + tests + dogfood.** Re-critique
   `critique_queue_entries_are_self_contained` (which currently has
   two open findings), close one accept + one reject, verify
   `--apply-findings` no longer surfaces them by default.

## Alternatives considered (and rejected)

- **Separate disposition log file** (`<id>.disposition` alongside
  `<id>.critique`). Rejected: two files per id is more complexity than
  the single-file mutation buys back. The .critique file is already
  the canonical store; adding a sidecar splits the source of truth.

- **Rewrite the critique on close, drop closed findings.** Rejected:
  loses the audit trail (which findings were rejected vs never
  existed?). The disposition field preserves history with minimal cost.

- **Multi-disposition history** (`disposition_log: Vec<Closure>`).
  Rejected for v0: YAGNI. Single-disposition is enough for the loop-
  closing use case. Can extend later if real workflows need it.

- **Direct file mutation by the skill orchestrator.** Rejected: violates
  the design split that SDK is sole writer of artifact files. The
  `--close-finding` CLI is the proper analog of `--submit-findings`.

- **No CLI, just rewrite-via-skill.** Rejected: the user must be able to
  close findings outside an active critique session (e.g., next morning,
  during a different orchestration). A CLI command makes the operation
  scriptable, agent-agnostic, and testable in isolation.

## Open questions (do not block decision)

1. **Note prompting cost.** Is it worth prompting for a note on every
   disposition? Could default to no-note and reserve note-prompting
   for `rejected` only (where the rationale matters most for future-
   us). Punt to first month of usage to decide.

2. **Apply-findings UX with mostly-closed critiques.** If a long-running
   project accumulates many closed findings, even `--include-closed`
   output gets noisy. Maybe needs a `--since <commit>` filter. Defer
   to actual scale problems.

3. **Cross-annotation closure.** If finding #2 on `foo` is "vocabulary
   inconsistent with sibling `bar`," and the user accepts it on `foo`
   by changing `foo`'s text — does that auto-close any related
   findings on `bar`? Probably no for v0 (too clever); revisit if
   patterns emerge.

4. **`Apply rewrite` for non-rephrasing categories.** Could the SDK
   ever propose a rewrite for `clarity` or `scope` findings? Today
   those categories deliberately don't ship `suggested_text` because
   the worker can't know the right replacement. A v2 critique pipeline
   could ask the worker to propose a rewrite on user request (separate
   subagent call). Out of scope for this slice.

5. **Closed-finding cleanup.** Should there ever be a `--gc` to drop
   closed findings older than X? Currently the design says no — the
   audit trail is the value. Revisit if .critique files balloon.
