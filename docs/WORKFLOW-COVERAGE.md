# Workflow-diagram coverage audit

Audit of `../../aretta-sdk/docs/diagrams/{01-lifecycle,02-state-map,03-verify-execution}.mmd` against the trycmd scenarios in `crates/aristo-cli/tests/cmd/_pending/`. Goal: every meaningful flow described by the diagrams has at least one test scenario.

**Status legend**

- ✅ covered — at least one scenario asserts this flow end-to-end
- ⚠ partial — flow is touched by some scenario but not isolated and asserted
- ❌ missing — no scenario exercises this flow

**Scope split.** The next week's implementation is **offline-only** — no server, no `aretta.dev`, no auth, no `aristos:` bindings. §1 covers everything we ship now; §2 collects the deferred server/auth flows (scenarios may already exist as `_pending/` files but their backing code waits for server work); §3 covers build-time integration (cargo features) which is on a separate axis.

---

## §1. Offline (in scope, next ~1 week)

### 1.1 — `01-lifecycle.mmd` (offline subset)

| Diagram node(s) | Flow | Status | Scenario(s) / gap |
|---|---|---|---|
| S→s1 + s2 → L→l3 + l5 | Setup → first stamp → first verify (free) | ✅ | `lifecycle_init_to_first_verify.md` |
| L→l1 → l3 | Edit-then-restamp chain (stamp surfaces drift) | ❌ | Missing — stamp tested standalone but not as a "edit code → see staleness flag rise" sequence |
| L→l2 (git commit → pre-commit hook → stamp + lint) | Hook wraps `aristo stamp` + `aristo lint` | ❌ | Needs `assert_cmd` + git fixture; not a trycmd scenario |
| L→l3 standalone | `aristo stamp` cycle/uniqueness/staleness | ✅ | `stamp_cycle_diagnostics.md` |
| L→l4 | `aristo lint --check` / `--fix` | ✅ | `lint_check_fail.md`, `lint_fix_restages.md` |
| L→l5 | `aristo verify --filter --rerun` orchestration | ⚠ | `verify_filter_rerun.md` shows flag forms but no isolated `verify=test` free baseline (only J4 downgrade context); see §1.3 |
| L→l6 | `aristo rename` atomic project-wide | ✅ | `rename_*.md` |
| H→h1+h2+h3+h4 | Pre-push ship (doc + graph + badge) | ✅ | `lifecycle_ship_with_doc_and_graph.md` |
| C→c1+c2+c3+c4 | CI gate sequence | ✅ | `lifecycle_ci_gates.md` |
| I→i1, i2, i3, i4 | Inspection per-command (status / list / show / graph) | ✅ | `status_full_output.md`, `list_*.md`, `show_*.md`, `graph_*.md` |
| I→i5 | `aristo review --filter` (free tier; local skill) | ❌ | Zero coverage of `aristo review` |
| I cluster as a chain | Debugging walkthrough: status → list → show → graph | ⚠ | Each command tested standalone; no chained debugging-walkthrough scenario |

### 1.2 — `02-state-map.mmd` (offline subset)

| Diagram edge(s) | Flow | Status | Scenario(s) / gap |
|---|---|---|---|
| `w_init` → conf, idx, spcs, dcs (4 edges) | `aristo init` creates ALL four state files | ⚠ | `lifecycle_init_to_first_verify.md` checks aristo.toml + dirs + hook; **does not assert `.aristo/index.toml` exists at init time** — open: does init create empty index, or does first stamp create it? Decide + test |
| `w_index` → idx | `aristo index --all` standalone regenerate | ❌ | TOOLS.md defines it; no scenario exists |
| `w_dev` → src | Developer edits source | n/a | Not a CLI flow |
| `w_stamp` → idx | `aristo stamp` writes hashes + B5b state + ids | ✅ | `stamp_cycle_diagnostics.md` (cycles); per-id stamping covered piecewise via lifecycle scenario |
| `w_vft` → spcs (mined spec) AND idx (status) | `aristo verify` (free, test) writes BOTH spec file AND index status | ⚠ | `verify_free_tier_downgrade.md` shows pipeline in J4 context; **doesn't isolate the spec-file write as primary observable** |
| `w_rename` → src + spcs + idx (atomic) | `aristo rename` writes all three places | ✅ | `rename_*.md` |
| `w_doc` → dcs | `aristo doc` writes per-annotation markdown + summary + graph | ✅ | `doc_*.md` (6 files) |
| Freshness preflight: idx → r_status, r_list, r_show, r_graph, r_review, r_audit, r_badge | J5 advisory across **all 7 readers** | ⚠ | `stale_index_preflight.md` covers show / graph / status / doc — but **not list / review / badge**. (audit is in §2.) |

### 1.3 — `03-verify-execution.mmd` (offline subset)

| Diagram path | `verify` × tier | Status | Scenario(s) / gap |
|---|---|---|---|
| `vlvl=false` → `noop` | `verify = false` (any tier) | ❌ | Missing — should be a one-line scenario asserting "skipped: documentation only" |
| `n_tier=Free` → `n_free` → `out_status` | `verify = "neural"` free (aristo-neural-verify skill, status only) | ❌ | Missing |
| `t_tier=Free` → `t_free` → `t_spec` → `t_feat` → `t_ct` → `out_status` | `verify = "test"` free, full pipeline (mine → spec write → cargo_verify feature → cargo test → status) | ⚠ | `verify_free_tier_downgrade.md` shows this in J4 context only. Need a clean baseline `verify=test` free scenario |
| `f_tier=Free` → `f_free_note` → `t_free` (J4 downgrade) | `verify = "full"` free → degrades to test | ✅ | `verify_free_tier_downgrade.md` |
| `rr=yes` → `keep` (force re-verify clean entries) | `--rerun` keeps already-verified entries | ⚠ | `verify_filter_rerun.md` shows the flag; **doesn't show before/after asserting the "would have skipped without --rerun" semantics** |
| `rr=no` → `skip` (default skip-clean) | Default: idempotent re-runs | ⚠ | Implicit. Missing scenario: "second `verify` run with no source change is a no-op" |

### 1.4 — Offline shopping list (write these into `_pending/`)

12 trycmd scenarios + 1 imperative integration test:

1. `init_creates_index_file.md` — diagram 02 `w_init` (decide if init writes empty index)
2. `index_standalone.md` — diagram 02 `w_index`
3. `verify_false_skipped.md` — diagram 03 `noop`
4. `verify_neural_free.md` — diagram 03 `n_tier=Free`
5. `verify_test_free_full_pipeline.md` — diagram 03 `t_tier=Free` (clean baseline, no J4 framing)
6. `verify_rerun_keeps_clean_entries.md` — diagram 03 `rr=yes` semantics
7. `verify_default_skips_clean_entries.md` — diagram 03 `rr=no` (default)
8. `stale_preflight_on_list.md` — diagram 02 freshness preflight
9. `stale_preflight_on_review.md` — diagram 02 freshness preflight
10. `stale_preflight_on_badge.md` — diagram 02 freshness preflight
11. `review_filter.md` — diagram 01 I→i5 (the only `aristo review` coverage we'd have)
12. `edit_then_stamp_surfaces_drift.md` — diagram 01 L→l1→l3 chain

Imperative (`assert_cmd` + git fixture, not trycmd):
- `pre_commit_hook_runs_stamp_and_lint` — diagram 01 L→l2

---

## §2. Server / auth (deferred — not in the next-week scope)

These flows depend on `aretta.dev`, the auth subsystem, or pre-existing `aristos:` bindings (which only come from `aristo sync`). Scenarios may already exist as `_pending/` files; their backing code waits for the server-side slice.

### 2.1 — `01-lifecycle.mmd` (server subset)

| Diagram node(s) | Flow | Status | Scenario(s) / gap |
|---|---|---|---|
| S→s3 | `aristo auth login` writes `.aristo/credentials` | ⚠ | Stubbed in `lifecycle_paid_sync_binding.md`; no standalone scenario |
| P→p1 | `aristo sync` first-bind | ✅ | `lifecycle_paid_sync_binding.md` |
| P→p2 (suggestions list / apply / reject) | Server-pushed annotation suggestions | ❌ | Zero coverage of `aristo suggestions` |
| P→p3 (sync --rebind branch) | `aristo sync --rebind aristos:<id>` after edit invalidated outcome | ❌ | Missing |
| P→p3 (unbind branch) | `aristo unbind aristos:<id>` (locally removes server-state) | ❌ | Missing |

### 2.2 — `02-state-map.mmd` (server subset)

| Diagram edge(s) | Flow | Status | Scenario(s) / gap |
|---|---|---|---|
| `w_auth` → creds | `aristo auth login` writes `.aristo/credentials` (gitignored) | ❌ | Missing |
| `w_vp` → idx (verified_outcome + status) | `aristo verify` (paid) writes signed outcome | ✅ | `b5b_state_verified.md` (assumes pre-existing bound annotation) |
| `w_sync` → idx (linked + verified_outcome) AND src (aristos: prefix) | `aristo sync` writes BOTH source AND index atomically | ⚠ | `lifecycle_paid_sync_binding.md` mentions both; doesn't isolate-and-assert the source rewrite |
| `w_unbind` → src (strips prefix) AND idx (removes binding) | `aristo unbind` reverses sync atomically | ❌ | Missing |
| `w_sugg` → src (inserts) AND idx (updates) | `aristo suggestions apply` writes both | ❌ | Missing |
| Freshness preflight: idx → r_audit | J5 advisory on `aristo verify --audit-only` | ❌ | Missing — audit-only itself is offline-runnable, but operates on server-produced state |

### 2.3 — `03-verify-execution.mmd` (server subset)

| Diagram path | `verify` × tier | Status | Scenario(s) / gap |
|---|---|---|---|
| `n_tier=Paid` → `n_paid` → `out_signed` | `verify = "neural"` paid (server HQ neural; certificate emitted with method=neural) | ❌ | Missing |
| `t_tier=Paid` → `t_paid` → `out_signed` | `verify = "test"` paid (server HQ mining + IP-safe bug report) | ❌ | Missing |
| `f_tier=Paid` → `f_paid` → `out_signed` | `verify = "full"` paid (server best-method) | ✅ | `b5b_state_verified.md` |

### 2.4 — B5b diagnostic states (require server-issued certificates to exist)

All of these `_pending/` scenarios already exist but require server-produced state to exercise:

- `b5b_state_verified.md` — happy path Certified outcome
- `b5b_state_stale.md` — body_hash drift after verification
- `b5b_state_orphan.md` — commit_hash not in this repo's history
- `b5b_state_forged_tampered.md` — sig fails any bundled key
- `b5b_state_forged_revoked.md` — sig valid but key revoked
- `b5b_shallow_clone_pending_deepen.md` — ancestry can't be confirmed in shallow CI checkout
- `verify_audit_only.md`, `verify_audit_only_check.md`, `verify_audit_only_strict.md` — offline cert validation

These ship with the server slice (or with bundled-test-keys infrastructure, whichever comes first).

### 2.5 — Server/auth shopping list (deferred; write later)

When the server slice lands:

- `auth_login_writes_credentials.md` — diagram 02 `w_auth`
- `auth_logout.md`
- `unbind_atomic.md` — diagram 02 `w_unbind` + diagram 01 P→p3
- `sync_rebind.md` — diagram 01 P→p3
- `suggestions_list.md`, `suggestions_apply.md`, `suggestions_reject.md` — diagram 01 P→p2 + diagram 02 `w_sugg`
- `verify_neural_paid.md` — diagram 03 `n_tier=Paid`
- `verify_test_paid.md` — diagram 03 `t_tier=Paid`
- `stale_preflight_on_audit.md` — diagram 02 freshness preflight on audit-only
- `badge_strict_publisher_provenance.md` — diagram 01 H→h4 strict variant (touches `aretta.dev/registry/`)

---

## §3. Build-time integration (separate axis)

Build-time integration is offline (no network) but lives in the cargo / proc-macro layer rather than the CLI. Tests need cargo-fixture projects, not trycmd.

| Diagram element (02) | Flow | Status | Test approach |
|---|---|---|---|
| `fc` (`aristo_check` feature) → compile-time validation | Proc-macro emits `compile_error!` for malformed annotations | ❌ | trybuild compile-fail tests in `crates/aristo-macros/tests/ui/` (slice 7, mockup 05/check_example.rs source) |
| `fv` (`aristo_verify` feature) → cargo test | Macro injects mined assertions from `.aristo/specs/<id>.spec` during cargo test | ❌ | Imperative `assert_cmd` test running cargo with fixture project |
| `fd` (`aristo_doc` feature) → cargo doc | Macro injects `#[doc = include_str!(...)]` for rustdoc | ❌ | Same — imperative test running `cargo doc --features aristo_doc` against fixture |

These slot in as future slices once `aristo-macros` has its first proc-macros (slice 6 in the post-compaction plan).

---

## How to use this document

1. When a slice lands a new CLI command, check this document for the relevant flow row(s) and **promote any covered `_pending/` scenarios into `active/` in the same commit**.
2. When a slice closes a "missing" gap, **add a `_pending/` scenario from §1.4 or §2.5** in the same commit (or one before it).
3. The audit was performed on commit `077b041` (slice 5 close-out). Re-run the audit when the diagrams or scenarios change substantially.
