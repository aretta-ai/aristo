# Review sessions — generic stateful triage substrate

**Status: PROPOSED 2026-05-18.** Generalized from the critique-disposition
proposal in `critique-finding-disposition.md`. That sibling doc captured
the critique-pipeline-specific loop-closing need; this doc captures the
generic substrate underneath. When this lands, the critique-disposition
work folds into it as the first non-trivial use case.

Third entry under `docs/decisions/`.

## Context

Several aristo workflows produce artifacts the user must triage before
they land:

- **Proof review** (verify pipeline) — neural proofs come back with
  verdicts the user walks through, plus suggested annotations on
  inconclusive verdicts the user accepts/rejects.
- **Critique review** (critique pipeline) — categorized findings the
  user accepts (and acts on), rejects (so they don't recur), or defers.
- **Authoring review** (future) — when an agent proposes new annotations
  while writing code, the user reviews each before committing.

Without explicit triage state, three failure modes recur:

1. **Slop drift.** Artifacts get committed without explicit sign-off;
   the user loses visibility into what was added.
2. **Zombie suggestions.** Rejected suggestions reappear on every run
   because nothing remembers they were rejected.
3. **Mid-review derailment.** The agent moves on to planning/coding
   while the user has a half-finished review open; context is lost on
   restart.

The skills currently handle this ad-hoc per pipeline (the
neural-verify skill has step 7, the critique skill has step 5).
Different shapes, different state management, no enforcement that the
agent stays in the review until exit.

## Decisions (proposed)

### D1. `Session` is the generic abstraction

A `Session` is a stateful triage context with a defined lifecycle:

```
session start  →  items presented  →  per-item decide  →  session exit
                                                              ↓
                              accepted → applied (source/index changes)
                              rejected → appended to rejection log
                              pending  → moved to backlog
                              (open)   → blocks `exit`; `--defer-undecided`
                                         moves them to pending; `abort`
                                         drops the whole session
```

Three first-class buckets at exit + a fourth implicit "open" state that
forces explicit user action before close.

### D2. Single active session, enforced mechanically

At most one session is active per workspace at a time. Active session id
lives in `.aristo/sessions/.active`. Three enforcement layers:

**Layer 1: SDK pre-checks at command boundaries.**

| Command class | If active session exists |
|---|---|
| Pure read (`show`, `list`, `status`, `session active`, `session status`) | always allow |
| Same-kind continuation (`session decide`, the same review skill) | allow |
| Different-kind session start (`session start critique-review` while proof-review active) | **block** unless target kind opts into nesting (default: disallow) |
| Pipeline writes that produce reviewable artifacts (`verify --apply-verdicts`, `critique --apply-findings`) | **block** with "active session must exit first" |
| Other aristo mutations (`stamp` etc.) | **block** with same message |
| Direct source edits via Edit tool | **warn** loudly via hook reminder; do not block (sometimes a side-channel fix is genuinely needed mid-review) |

The block is a real exit-non-zero refusal, not a courtesy warning.

**Layer 2: Claude Code hook for context injection.**

A `UserPromptSubmit` hook calls `aristo session active`. If a session id
is returned, it prepends a kind-agnostic system reminder:

```
<system-reminder>
You are currently in an aristo review session:
  id:      <session-id>
  kind:    critique-review
  subject: src/critique/pending.rs
  items:   3 open, 2 decided, 0 deferred

While this session is active:
  - You cannot start a different review session, run aristo
    mutation commands, or move to planning/implementation.
  - You may: continue this review, run read-only aristo commands,
    have discussion with the user.
  - Direct file edits via Edit are allowed but discouraged
    (any unrelated change should wait for clean exit).

Commands:
  aristo session status                       # peek state
  aristo session decide --item <ref> --bucket <accepted|rejected|pending>
  aristo session exit                         # strict close (all items decided)
  aristo session exit --defer-undecided       # close, move open items to pending
  aristo session abort                        # destructive cancel

To continue this review, invoke /aristo-critique.
</system-reminder>
```

This makes the state *visible in every turn* — survives Claude Code
restart and context compaction. The agent cannot forget.

**Layer 3: Skill body discipline.**

Each review-capable skill has a "step 1 — check for active session" that
calls `aristo session active`:
- empty → start a fresh session, walk through normally
- active session of MY kind → resume from where the user left off
- active session of DIFFERENT kind → refuse: tell user to exit first

The skill body is the per-kind specialist that knows how to render
items, what `AskUserQuestion` menus to offer, and how the three buckets
map to source/index changes for *this* kind.

### D3. CLI surface

```
aristo session start <kind> --subject <focal-ref> [--allow-nesting]
aristo session active                              # prints id or empty
aristo session status                              # bucket counts + open items
aristo session decide --item <ref> \
                      --bucket <accepted|rejected|pending> \
                      [--note "..."]
aristo session exit                                # strict: errors if open items
aristo session exit --defer-undecided              # moves open → pending; closes
aristo session abort                               # destructive cancel
aristo session list                                # active + recent (default 10)
```

### D4. Force-close = defer-undecided, not drop

`exit --defer-undecided` moves all open items to the pending backlog,
then closes the session. NOT a silent drop. The user is saying "I want
out, deal with these later" — the items survive in the backlog where
the next session will surface them.

`abort` is the rare destructive escape hatch ("forget this whole thing
ever happened"). It drops the session entirely with no decisions
recorded. Confirmation prompt before it fires.

### D5. Git tracking: all of `.aristo/sessions/` is gitignored

```
.aristo/sessions/
├── .active                    # gitignored — pointer to active session id
├── active/<id>.session.toml   # gitignored — in-flight session state
├── closed/<id>.session.toml   # gitignored — audit trail of past sessions
├── rejections.log             # gitignored — append-only JSONL
└── backlog/<kind>.toml        # gitignored — pending items by kind
```

Single `.gitignore` line: `.aristo/sessions/`. The *effects* of
approvals (source edits, new annotations, index updates) go to git via
normal code commits. The session record itself stays local — personal
audit trail, not project artifact.

Rationale: keeps PR diffs clean (no session metadata churn), avoids
leaking in-flight review state across team members, and treats sessions
correctly as user-personal workflows rather than project artifacts.

### D6. Per-kind extensibility via trait

```rust
trait SessionKind {
    const NAME: &'static str;
    const NESTING_POLICY: NestingPolicy;          // Disallow | AllowKinds(&[&str])

    /// Per-kind item type. Critique's is `(critique_id, finding_index)`;
    /// proof-review's is `(annotation_id, action_path)`; etc.
    type Item: Serialize + DeserializeOwned;

    /// On accept: apply the item's effect (source edit, index update, etc.)
    fn on_accept(item: &Self::Item, note: Option<&str>, ws: &Workspace) -> CliResult<()>;

    /// On reject: derive the rejection-log entry. Used for the auto-reject
    /// filter on future sessions.
    fn on_reject(item: &Self::Item, note: Option<&str>) -> RejectionEntry;

    /// On pending: derive the backlog entry.
    fn on_pending(item: &Self::Item, note: Option<&str>) -> BacklogEntry;

    /// Match function for auto-rejecting items in future sessions. Given a
    /// candidate new item, return true if it's "the same" as some prior
    /// rejection. Per-kind heuristic — critique uses category+rationale
    /// similarity; proof-review uses focal-id + gap-shape.
    fn matches_prior_rejection(item: &Self::Item, prior: &RejectionEntry) -> bool;
}
```

v0 kinds:
- `CritiqueReviewSession` — items are `(critique_id, finding_index)`
- `ProofReviewSession` — items are `(proof_id, suggested_annotation_index | "verdict")`

The substrate (`.active` enforcement, exit semantics, rejection log, backlog
plumbing) is fully generic over the trait.

### D7. Soft-filter rejections via separate menu

Auto-rejection NEVER silently drops; it just routes to a different menu.

On session entry, the orchestrator computes four item populations:

| Population | Source |
|---|---|
| `open` | new items not matching any prior rejection |
| `auto-rejected` | new items matching some prior rejection |
| `backlog` | items deferred from prior sessions of this kind |
| `closed-this-session` | (resume only) already decided in this run |

The opening `AskUserQuestion` offers:

```
Question: Start critique review for <subject>?
Options:
- Walk through N open findings              — the new stuff to triage
- Review M auto-rejected (filtered)         — re-look at suggestions previously rejected
- Review K pending from backlog             — items deferred earlier
- Skip — exit without changes               — `aristo session abort`
```

Auto-rejected items don't clutter the main flow but stay one click away.
Selecting "Review M auto-rejected" walks them with the standard per-item
menu; the user can accept, re-reject, or pending them like any other
item (re-rejecting refreshes the rejection-log entry's timestamp).

### D8. Hook install + uninstall via `aristo install-skills`

Per-project (not user-level): writes to `<workspace>/.claude/settings.json`
so the hook only fires when working in an aristo workspace.

- `aristo install-skills [--agent <agent>] [--user]` →
  - installs skill manifests (existing behavior)
  - PLUS: adds `UserPromptSubmit` hook entry that calls `aristo session active`
- `aristo uninstall-skills [--agent <agent>] [--user]` →
  - removes skill manifests (existing behavior)
  - PLUS: removes the hook entry cleanly

The hook content is a tiny shell snippet (one `aristo session active` call,
piped to a small formatter). No skill-specific metadata embedded in the
hook itself.

Existing intent `install_skills_scope_symmetry` extends to cover the hook
install/uninstall — same symmetry guarantees apply.

### D9. Backlog surfaces in two places

- **Inside each review session's opening menu** (per D7) — passive,
  one click to review.
- **`aristo status` output** — passive count:
  `Backlog: 3 critique findings, 1 proof review (most recent: 2 days ago)`

User can manually trigger backlog-only review by invoking the skill with
a `backlog` argument (skill maps to `aristo session start <kind> --subject backlog`).

## Lifecycle file shape

```toml
# .aristo/sessions/active/01J5K9N7-critique-review.session.toml
schema_version = 1
id = "01J5K9N7-critique-review"
kind = "critique-review"
subject = "src/critique/pending.rs"
started_at = "2026-05-18T13:00:00Z"
started_by = "aristo-critique skill"
nesting_policy = "disallow"
state = "active"     # active | closed | aborted

[[items]]
ref = "critique_queue_entries_are_self_contained:0"
status = "open"      # open | accepted | rejected | pending

[[items]]
ref = "critique_queue_entries_are_self_contained:1"
status = "accepted"
note = "Will tighten the annotation in follow-up commit"
closed_at = "2026-05-18T13:05:23Z"
```

On close: file moves to `.aristo/sessions/closed/`, gains
`closed_at`, `exit_kind` (`exit` | `exit-defer-undecided` | `abort`),
and bucket-count summary.

```jsonl
# .aristo/sessions/rejections.log (append-only)
{"ts":"2026-05-18T13:05:00Z","kind":"critique-review","item_ref":"critique_queue_entries_are_self_contained:0","note":"Annotation is intentionally narrative","fingerprint":{"category":"clarity","rationale_sketch":"defensive_commentary"}}
```

The `fingerprint` is per-kind structured data the kind's
`matches_prior_rejection` uses to recognize "this is the same suggestion
the user already rejected."

## Implementation sequence

1. **`aristo-cli/src/session/`** — generic substrate. Session state
   types, `.active` pointer, atomic write helpers, rejection-log append,
   backlog read/write. ~400 LOC.
2. **`aristo session` CLI surface.** Subcommands: start, active, status,
   decide, exit, abort, list. ~200 LOC.
3. **SDK pre-check integration** — at the top of every mutating
   `aristo` command's `run`, call `session::guard_for_command(cmd_kind)`.
   Existing tests need active-session-blocks-X cases.
4. **Hook install integration** — `install_skills.rs` writes the hook
   entry; `uninstall_skills.rs` removes it. Idempotent both directions.
5. **CritiqueReviewSession impl** — first concrete kind. Replaces the
   ad-hoc disposition mechanism from
   `critique-finding-disposition.md`.
6. **ProofReviewSession impl** — second concrete kind. Replaces step 7
   of `aristo-neural-verify` skill body.
7. **`aristo-critique` skill body rewrite** — step 1 checks for active
   session; step 5 wraps in `session start`/`session decide`/`session exit`.
8. **`aristo-neural-verify` skill body rewrite** — analogous changes
   to its step 7.
9. **`aristo status` integration** — backlog counts in the summary.
10. **Build + tests + dogfood.** Re-critique an annotation, walk
    through findings with accept/reject/defer, verify
    `--apply-findings` and re-running critique honor the disposition
    state.

## New intents to author alongside (during implementation, not after)

- `session_is_single_active_per_workspace`
- `session_exit_strict_blocks_open_items`
- `session_exit_defer_undecided_moves_open_to_pending_never_drops`
- `session_abort_is_destructive_drops_all_decisions`
- `session_active_pointer_is_workspace_local_not_committed`
- `sdk_blocks_mutations_while_session_active_except_direct_edit`
- `auto_rejection_routes_to_separate_menu_never_silently_drops`
- `install_skills_installs_hook_at_workspace_scope`
- `uninstall_skills_removes_hook_cleanly`

Plus the per-kind impl docstrings.

## Relationship to prior proposals

This generalizes and supersedes `critique-finding-disposition.md` (which
becomes the `CritiqueReviewSession` impl under this substrate). The
disposition doc's D1-D5 map to specific behaviors of `CritiqueReviewSession::on_accept` etc.

## Alternatives considered (and rejected)

- **Per-pipeline ad-hoc disposition** (the original disposition proposal).
  Rejected: every new review type re-implements the same loop. Substrate-
  generic is cleaner.
- **Pure convention without SDK enforcement.** Rejected: agents drift.
  The whole point is mechanical guardrails the user can rely on.
- **Sessions tracked in `~/.aristo/`, not per-workspace.** Rejected: a
  user often has multiple workspaces open; per-workspace state is the
  natural granularity.
- **Allow nesting by default.** Rejected: nested reviews are
  confusing and the user's framing was explicit ("clean exit"). Make
  nesting per-kind opt-in.
- **`Drop` instead of `Abort`.** Rejected: `abort` is clearer about the
  destructive semantic; `drop` reads as soft.
- **Strip rejected/pending from closed session file before commit, keep
  approved-only audit committed.** Rejected per user feedback: all
  session metadata is local; only the *effects* of approvals (source
  changes) go to git.

## Open questions (do not block decision)

1. **Rejection-log fingerprint shape per kind.** What exactly identifies
   "the same finding" for auto-rejection matching? Critique: probably
   `category + normalized(rationale[:100])`. Proof-review: probably
   `focal_id + gap.suggested_annotation.text_hash`. Refine after
   first month's usage.

2. **Backlog accumulation cap.** If a user defers 50 critique findings
   and never reviews the backlog, the menu gets crowded. Probably
   per-kind cap with oldest-first eviction at ~30. Defer to first
   scale problem.

3. **Cross-machine session.** A user working on the same project from
   laptop + desktop would currently lose sessions when switching
   machines (since `.aristo/sessions/` is gitignored). If this becomes a
   real workflow, add `aristo session export <id>` and
   `aristo session import <file>` as escape hatches. Out of scope for v0.

4. **Nesting use cases.** Are there *any* scenarios where nested
   sessions make sense? `author-review` -> proof-review on the
   just-authored intent could be one. Defer until a concrete use case
   demands it; default is disallow.

5. **`aristo session abort` confirmation flow.** Native CLI confirm
   prompt vs `--yes` flag vs typed-confirmation phrase. Probably native
   confirm with `--yes` to skip. Decide at implementation time.
