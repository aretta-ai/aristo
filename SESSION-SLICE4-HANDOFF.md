# Slice 4 — Session handoff for canon-and-matching SDK work

**Purpose:** brief the next Claude Code session enough to pick up Slice
4 (the `aristo` SDK side of the §13 canon-and-matching design) without
re-reading the entire design archive. The meta-workspace's
`docs/mockups/13-canon-and-matching/AGENT-BRIEFING.md` is still the
canonical entry-point — this file is the *continuation* note recording
where the previous session stopped.

**Status as of this handoff:** 6 of 12 PRs complete, all committed
locally to `main`, all green. Next up: PR #7 (accept path) — the
biggest PR in the slice, which is exactly why we're handing off.

## Read these first (in this order)

1. **`/Users/sushantd/projects/CampanileSkyForge/aretta-meta-workspace/docs/mockups/13-canon-and-matching/AGENT-BRIEFING.md`**
   — slice ownership map + required-reading list. You are Slice 4
   (this `aristo/` repo).
2. **`CLAUDE.md`** in this repo — the working agreement (§4
   test-first, §12 specs-are-truth, §12A promote-at-slice-start, §6
   full-check gating). Load-bearing.
3. **`CHANGELOG.md` `[Unreleased]` section** — what's landed so far,
   with detailed per-PR commentary. Most recent entries at the top.
4. **This file** — picks up where the previous session stopped.

## Where things stand

### Git commit log (`git log --oneline` against `main`)

```
<HEAD>  feat(critique): canonicalize category + surface canon matches in apply-findings   [PR #6]
        feat(cli): aristo stamp canon-match integration + 9 e2e scenario tests             [PR #5]
        feat(core): canon::cache — .aristo/canon-matches.toml schema + atomic I/O          [PR #4]
        feat(cli): aristo auth {login, status, logout} + 13 e2e integration tests          [PR #3.3]
        feat(core): HttpCanonClient + 9 end-to-end integration tests                       [PR #3.2]
        feat(core): canon::auth — token resolution + credentials persistence               [PR #3.1]
        feat(core): add NoopCanonClient + MockCanonClient impls                            [PR #2.2]
        feat(core): add aristo_core::canon module — wire types + CanonClient trait         [PR #2.1]
        feat(core): add [canon] config section with enabled + threshold knobs              [PR #1.2]
        feat(core): add kanon: id namespace + extend rename rejection to cover it          [PR #1.1]
```

Plus one meta-workspace commit `docs(canon): defer verification execution to Phase 2; normalize SDK artifact to TOML`.

**Nothing pushed.** All commits are local. The user is explicit: "No need to push or make an explicit PR. Continue."

### Test inventory

- **~117 unit tests** across `aristo-core::canon` (types / clients /
  http / cache / auth) and `aristo-cli::commands::canon::runner`.
- **34 end-to-end scenario tests** across three integration files:
  - `crates/aristo-core/tests/canon_http_e2e.rs` — 9 tests
    (`HttpCanonClient` against a `std::net::TcpListener` mock server).
  - `crates/aristo-cli/tests/auth_command.rs` — 13 tests (real
    `aristo auth {login,status,logout}` subprocess + isolated
    `HOME`/`XDG_CONFIG_HOME`/`ARETTA_TOKEN`).
  - `crates/aristo-cli/tests/canon_stamp_command.rs` — 9 tests
    (real `aristo stamp` against `MockCanonClient` fixtures; maps to
    cli-sessions.md Flow 1/2/6 + opt-outs).
  - `crates/aristo-cli/tests/canon_critique_command.rs` — 3 tests
    (real `aristo critique --apply-findings` surfacing canon matches
    from `canon-matches.toml`).

Run before any commit: `cargo fmt --all --check && cargo clippy
--workspace --all-targets -- -D warnings && cargo test --workspace`
(per CLAUDE.md §6).

### Cli-sessions.md scenarios — what's covered

| Flow | Scenario | Status |
|---|---|---|
| 1 | High-confidence stamp surface | ✅ PR #5 |
| 2 | Free-tier nudge | ✅ PR #5 |
| 3 | Reviewing the match in critique session | ⚠️ Partial — read-only surface in PR #6; the interactive `[a]ccept/[r]eject/[d]etail` UI is not yet wired (the existing review-session substrate from slice 27.5 handles those; PR #7 wires canonicalize findings into that session_kind) |
| 4 | aristos: tier accept (text rewrite + prefix application) | ⏳ **PR #7 (next)** |
| 5 | kanon: tier accept | ⏳ **PR #7 (next)** |
| 6 | Server unreachable graceful degradation | ✅ PR #5 |
| 7 | Rejection survives until text changes | ⏳ PR #7 (`rejected_matches` write-back) |
| Trust card | `aristo show <kanon\|aristos:id>` | ⏳ PR #10 |
| `aristo canon show/list/refresh` | | ⏳ PR #8 |
| `aristo canon unbind/request-verify` | | ⏳ PR #9 |
| `aristo status` canon health | | ⏳ PR #11 |

## Deviations & deferrals flagged during the slice

### Carried-forward follow-ups

1. **PR #6 did not wire a canon API call into `aristo critique`**
   itself (with broader `threshold_critique = 0.65`). Current behavior:
   stamp populates `canon-matches.toml` at 0.85 threshold; critique
   reads what stamp produced. L3's design says critique should *also*
   call the API to surface lower-confidence candidates that stamp
   filtered out. **Recommend folding this into PR #7** (since accept-
   path is in critique anyway) OR as a small follow-up commit after
   PR #7 lands.

### Design-archive adjustments user has made mid-slice

The user has been editing the design archive (`docs/mockups/13-canon-and-matching/*`
+ `docs/diagrams/*`) during the slice. Watch for in-flight edits via
system-reminders. Confirmed changes already absorbed by current
implementation:

- `entry-format.md` got an `alternative_phrasings` field (server-side
  only; explicitly never in API response — `CanonMatch` types stay
  as-is).
- `13-canon-routing.mmd` made the KNN matching stack explicit (OpenAI
  `text-embedding-3-large` + canon-vectors.db). Server-side only.
- `aristo canon show <id>` access policy was relaxed: no match-history
  gate; paid-tier auth + rate limit is sufficient. Already reflected in
  `CanonClient::get_entry` docstring (commit `628410e`).

### Phase 1 deferrals (already-resolved, don't re-litigate)

Per `docs/mockups/13-canon-and-matching/_deferred/verification-execution.md`:

- **No `verified_outcome` writes** on accept. PR #7 lands
  `BindingState::Bound { linked }` only; `Certified` is Phase 2.
- **No verification-execution endpoint** call from accept-path.
- **Trust card has no per-user Verification block** — only the
  canon entry's `backed_by` (i.e. "Aretta's commitment to verify",
  not "currently verified").

If the user changes their mind on any of these, surface as a question;
don't assume.

## PR #7 — Accept path (the next big PR)

### Goal

Wire user-accepted canonicalize findings (from PR #6's
`canon-matches.toml::pending_matches`) into atomic source-rewrite +
prefix-application + index update + cache update. Lands Flow 4
(`aristos:`-tier accept) and Flow 5 (`kanon:`-tier accept) from
cli-sessions.md.

### What "accept" means here

When the user runs `aristo critique --apply-findings` (PR #6 already
surfaces canonicalize findings; PR #7 adds the **mutation** layer
that's currently absent):

1. **Source rewrite.** The annotation's body (e.g.,
   `#[aristo::intent("each cell should be written exactly once per
   page edit")]`) is rewritten to use `id = ...` + `text = ...`
   keyword args with `id` carrying the canon prefix and `text` carrying
   the canonical phrasing — see Flow 4 / Flow 5 in
   `docs/mockups/13-canon-and-matching/cli-sessions.md` for the exact
   diff shape. **Atomic per-file write** (temp-then-rename).
2. **Prefix application.** The annotation's id (was local readable,
   stamp-assigned `aret_*`, etc.) is replaced everywhere by
   `aristos:<canon_id>` (if `prefix_tier == Aristos`) or
   `kanon:<canon_id>` (if `prefix_tier == Kanon`).
3. **Index update.** The entry in `.aristo/index.toml` is re-keyed
   under the new id, `BindingState` transitions `Local` →
   `Bound { linked: <linked_arta_id> }`.
4. **Cache update.** The `pending_matches[..]` entry for this
   annotation moves to `accepted_matches[..]` (renamed entry key in
   the cache too — same id transition as in the index).
5. **Reject path.** When the user declines a finding via
   `aristo session decide --bucket rejected`, the pending match
   moves to `rejected_matches[..]` pinned by
   `(canon_id, text_hash)` so it doesn't re-surface until annotation
   text changes (L5 invalidation).

### Where to put it + what to reuse

- **Reuse `aristo rename`'s span-substitution machinery.** See
  `crates/aristo-core/src/walk/scan_ids.rs::scan_id_occurrences` and
  `crates/aristo-cli/src/commands/rename.rs` (apply order:
  source first → artifacts → index LAST per the `apply_plan` intent).
  The span-substitution primitive is already battle-tested by the
  rename tests; lift it (don't reimplement).
- **Lives in:** `crates/aristo-cli/src/commands/canon/accept.rs`
  (new) — sibling of `runner.rs` already added in PR #5. Keep the
  module structure parallel: a single `pub(crate) fn
  apply_canonicalize_findings(ws, index, accepted_finding_ids)`
  entry point called from `apply.rs::run_apply_findings` after the
  existing critique-finding accept loop.
- **Don't extend the rename command itself.** The CS13 design retires
  `aristo rename` for canon prefixes. The accept path is its own
  primitive; rename stays bare→bare + opaque-promotion only.

### Source-rewrite shape (Flow 4 expanded)

Before:
```rust
#[aristo::intent("each cell should be written exactly once per page edit")]
fn edit_page(...) { ... }
```

After (aristos: tier):
```rust
#[aristo::intent(
    id     = "aristos:cell_written_exactly_once_per_page_edit",
    text   = "edit_page writes each cell exactly once",
    verify = "neural",
)]
fn edit_page(...) { ... }
```

**Two source changes** here, not one: text gets the canonical phrasing,
id gets the canon prefix. Both happen atomically in a single file
rewrite. If a `verify = ...` was already present, preserve it; if not,
write the default per the existing intent macro defaults.

### Atomicity / ordering contract

Same as rename (it's the same problem):

1. Compute the full plan (which source spans, which index update,
   which cache update) without writing.
2. Write source files first (each one atomic temp-then-rename;
   multiple files are sequentially atomic but not cross-file
   atomic — that's accepted).
3. Re-write `.aristo/index.toml` atomically (re-keyed under the new
   id + binding state).
4. Re-write `.aristo/canon-matches.toml` atomically (moving pending
   → accepted).

If any step fails, `aristo stamp` on the next invocation reconciles
(detects the source as having new ids that the index doesn't carry
yet; rebuilds index entries accordingly). Same "best-effort
recoverable" contract as rename — see the intent on
`apply_plan` (`rename.rs:48`).

### E2E tests to write (per §12A spec-first)

Live in `crates/aristo-cli/tests/canon_accept_command.rs` (new file).
Pattern matches `canon_stamp_command.rs`: tempdir workspace + isolated
`HOME`/`XDG_CONFIG_HOME` + `ARISTO_CANON_FIXTURE`. Pre-populate
`canon-matches.toml` (or run stamp first to populate it) and then run
`aristo critique --apply-findings` with the canon finding accepted.

**Minimum scenarios for Flow 4 + Flow 5:**

1. `accept_aristos_tier_rewrites_source_and_applies_prefix` (Flow 4).
2. `accept_kanon_tier_rewrites_source_and_applies_kanon_prefix` (Flow 5).
3. `accept_updates_index_binding_state_to_bound`.
4. `accept_moves_pending_to_accepted_in_cache`.
5. `accept_does_not_call_verification_endpoint` (Phase 1 invariant).
6. `reject_moves_pending_to_rejected_with_text_hash_pinned`.
7. `reject_does_not_resurface_unless_annotation_text_changes` (Flow 7).
8. Atomicity: partial-failure-leaves-recoverable-state.

### Gotchas already surfaced (will trip the new session too)

1. **`aret_*` IDs are non-deterministic between stamp runs** for un-id'd
   annotations. Test source must pin an explicit `id = "..."` to
   compare state across stamps. Document at the test source constant
   the same way `canon_stamp_command.rs::SOURCE_WITH_ONE_INTENT` does.
2. **`std::env::set_var` requires unsafe** — the workspace forbids
   `unsafe_code`. Use the `_with_*` injection variants of any function
   that reads env vars. See `canon::auth::resolve_with` for the
   pattern, and `auth_command.rs::isolated()` for the subprocess
   pattern (env-clear + re-add).
3. **`ureq 3.x` defaults non-2xx to `Err(StatusCode)`** — already
   configured `.http_status_as_error(false)` in `HttpCanonClient::new`,
   but if you spin up any new ureq agents, repeat the config.
4. **`canon::auth::save`/`clear`/`resolve` all read `$XDG_CONFIG_HOME`
   + `$HOME`** from process env (fixed in PR #3.3). Tests use the
   `_with` variants with explicit overrides.
5. **`include_str!` from `aristo/` can't reach
   `aretta-meta-workspace/docs/`** — the meta-workspace lives as a
   sibling, not a parent, and Cargo resolves to canonical paths. Test
   fixtures get reconstructed inline (see
   `canon::cache::tests::worked_example_matches_locked_sample_shape`).

### Definition-of-done for PR #7

Per CLAUDE.md §6 + Definition of Done at the bottom of CLAUDE.md, all
five must hold:

- [ ] A test demonstrates the new behavior (every accept-path branch
      has an e2e test).
- [ ] `cargo fmt --check && cargo clippy -- -D warnings && cargo test
      --workspace` all green.
- [ ] CHANGELOG.md `[Unreleased]` has a bullet describing the change.
- [ ] Semantic commit message; one logical change (PR #7 = accept).
- [ ] Committed (push optional per user's explicit instruction).

### Estimated scope

400–600 LOC in `commands/canon/accept.rs` (lifting + adapting
rename's span-substitution + plan-compute pattern); 200–300 LOC of
e2e tests; 1 CHANGELOG bullet; 1 commit.

## Remaining PRs (8–12) — abridged

PR #8 (`aristo canon {show, list, refresh}`), PR #9 (`aristo canon
{unbind, request-verify}`), PR #10 (`aristo show` trust card rendering),
PR #11 (`aristo status` canon health), PR #12 (version migration on
stamp — patch + minor). The original roadmap in the user's first
message (long since rolled out of context) had detailed scope for
each. The CHANGELOG entry for PR #5 documents the runner module's
ownership for the canon CLI subcommand family — PR #8/#9 add
sibling files under `commands/canon/`.

PR #11 (`aristo status` canon health) probably needs to surface
**`scopes:` line** for DP/Enterprise per `cli-sessions.md`'s
status block — the cache's `effective_scopes` from the last
match response is the data source.

PR #12 (version migration) is the only PR that needs to read the
catalog's `INDEX.yaml` resolution — but per CS12, the **server**
serves the active version, so this PR is just: detect a cached
`version` that's no longer in match responses, and either refresh
(patch bump — quiet update) or auto-unbind (minor bump — strip
prefix, clear index entry, surface critique finding for re-binding).
Patch-bump auto-refresh is straightforward; minor-bump auto-unbind
reuses PR #9's unbind primitive.

## Files / paths to know

| Topic | Location |
|---|---|
| Slice 4 design archive (read-only) | `../aretta-meta-workspace/docs/mockups/13-canon-and-matching/` |
| Canon strategy (CS1–CS13) | `../aretta-meta-workspace/docs/launch/canon-strategy.md` |
| Deferred verification surfaces | `../aretta-meta-workspace/docs/mockups/13-canon-and-matching/_deferred/verification-execution.md` |
| Wire types | `crates/aristo-core/src/canon/types.rs` |
| Client trait + errors | `crates/aristo-core/src/canon/client.rs` |
| HTTP impl | `crates/aristo-core/src/canon/http_client.rs` |
| Mock + Noop | `crates/aristo-core/src/canon/{mock_client,noop_client}.rs` |
| Auth (token resolve + persist) | `crates/aristo-core/src/canon/auth.rs` |
| Cache schema + atomic I/O | `crates/aristo-core/src/canon/cache.rs` |
| Shared canon-step runner | `crates/aristo-cli/src/commands/canon/runner.rs` |
| `aristo auth` CLI | `crates/aristo-cli/src/commands/auth.rs` |
| `aristo stamp` integration site | `crates/aristo-cli/src/commands/stamp.rs::run_canon_step_for_stamp` |
| `aristo critique --apply-findings` canonicalize render | `crates/aristo-cli/src/commands/critique/apply.rs::print_canonicalize_findings` |
| `aristo rename` span-substitution to reuse for accept | `crates/aristo-cli/src/commands/rename.rs::apply_plan` + `crates/aristo-core/src/walk/scan_ids.rs::scan_id_occurrences` |
| All canon e2e tests | `crates/aristo-{core,cli}/tests/canon_*.rs` + `auth_command.rs` |

## Working-style notes the user has expressed

- "Continue as planned" / "Yes, go ahead" — light-touch directives;
  proceed with the planned PR sequence without re-confirming.
- "No need to push or make an explicit PR. Continue." — keep commits
  local; don't `git push`; don't open PRs.
- Asks pointed questions about testing (PR #3 → "any end-to-end
  scenario tests with mocks?") — user values e2e coverage; lean into
  it.
- Edits the design archive mid-session — watch for system-reminders
  about modified files in the meta-workspace. The user's edits are
  authoritative; adapt the implementation, never push back unless
  there's a hard implementation blocker.
- Surfaced design ambiguities have been answered crisply:
  - Verification execution → Phase 2 (deferred).
  - All `.aristo/*` artifacts → TOML.
  - `aristo auth login` → in scope for this slice.

## Resume checklist for the new session

1. `git -C aristo log --oneline | head -15` — verify the commit list
   matches the table above.
2. `cd aristo && cargo test --workspace` — should pass clean.
3. Re-read `CLAUDE.md` (this repo) — the working agreement.
4. Skim `CHANGELOG.md` `[Unreleased]` for the last 6 entries — they
   document the current state in detail.
5. Read the "PR #7" section in this file. Confirm understanding of
   the accept-path contract.
6. Read `crates/aristo-cli/src/commands/rename.rs::apply_plan` +
   `crates/aristo-core/src/walk/scan_ids.rs` to understand the
   span-substitution primitive you'll lift for the canon accept path.
7. Skim `crates/aristo-cli/tests/canon_stamp_command.rs` — that's the
   pattern your `canon_accept_command.rs` tests will follow.
8. Start PR #7. Test-first per CLAUDE.md §4.

Good luck. The substrate is solid; PR #7 is the keystone that lets
end users actually *accept* canon matches and bind their source.
