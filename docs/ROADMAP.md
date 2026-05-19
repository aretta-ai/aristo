# Aristo SDK — Phase 1 roadmap

Phase 1 = the offline Rust SDK MVP. Server, auth, `aristos:` bindings, and per-language SDKs are out of scope (see `WORKFLOW-COVERAGE.md` §2 and the deferred lists at the bottom of this file).

The roadmap is organized into **8 milestones (A–H)**, totalling **30 numbered slices** continuing from the slice 1–5 series already shipped. Each milestone closes with a workspace version bump and a git tag — so the project gains a real release cadence from now on instead of one big-bang publish at the end.

**MVP = end of milestone H = v0.1.0.**

**Scope adjustment 2026-05-17:** slices 24, 25, 26 (`verify="test"` and `verify="full"` paths) were originally part of milestone E. They are deferred to post-MVP pending a tight design pass on the specification schema + injection mechanism (which are coupled and need the open questions in `docs/deferred/verify-test-design.md` settled first). Milestone E ships with slices 22 + 23 only and tags v0.0.6. MVP is 27 slices instead of 30 (slices 27–35, minus the 3 deferred).

## How this roadmap relates to `CLAUDE.md`

`CLAUDE.md` is the source of truth for **how we work** (commit size, semantic messages, test-first, full-check gating, plan-driven, annotation discipline, release cadence). This roadmap is the source of truth for **what we build, in what order**. Don't duplicate rules between them — when they disagree, `CLAUDE.md` wins.

Two `CLAUDE.md` rules are load-bearing for this roadmap:

- **§10 — Annotation discipline.** From slice 6 onward, we annotate Aristo's own source. The authoring skill takes over from slice 13.
- **§11 — Release cadence.** Each milestone closes with a `chore(release): v0.0.X` commit + `git tag v0.0.X`. Versions are dense and small (v0.0.2 → … → v0.1.0).

---

## Milestone A — Macro foundation → v0.0.2

Goal: `#[aristo::intent(...)]` and `#[aristo::assume(...)]` compile and validate. We can hand-write annotations on any file from this point onward.

| Slice | Deliverable | Notes |
|---|---|---|
| **6** | `aristo-macros` proc-macros | `#[intent]` / `#[assume]` attribute forms, `intent!{}` / `assume!{}` function-like forms, no-op pass-through expansion, argument parsing matching `IntentEntry` / `AssumeEntry` shape. Re-exported from the `aristo` meta crate. (mockup 01/02) |
| **7** | `trybuild` UI tests — compile-pass | Cases for all four surface forms across fn / impl / struct / trait / mod / module-root. Closes pending task #10. |
| **8** | `aristo_check` cargo feature | Single-annotation compile-time validation: bad `verify` value, empty text, `verify` on `assume`, reserved-prefix id (`aret_*`, `aristos:*`), malformed id. Pulled forward from milestone H — anything that helps us write and check intent should land as early as possible. Trybuild compile-fail matrix grows here. |
| **9** | CLI dispatch + workspace discovery | `clap` subcommand router, workspace-root finder (`Cargo.toml` + `.aristo/`), shared parsers for `--filter` / `--check` / `--strict` / `--json`, error type + exit codes. Removes the "not yet implemented" stub binary; rewrites `binary_smoke` as the canary for the new dispatch layer. |

**Closes:** pending task #10. **From here:** annotate-as-we-go (rule above).

## Milestone B — Bootstrap → v0.0.3

Goal: agent-assisted authoring works on our own dev machine. The dogfood loop activates.

| Slice | Deliverable | Notes |
|---|---|---|
| **10** | `aristo init` | Writes `aristo.toml`, empty `.aristo/index.toml`, `.aristo/{specs,doc}/`, `.git/hooks/pre-commit` placeholder, `.github/workflows/aristo.yml` starter. **Prints** the `aristo = "..."` line for the user's `Cargo.toml` by default; `-f` / `--force` actually modifies `Cargo.toml`. Idempotent. Promotes `init_creates_index_file.md`. |
| **11** | `aristo lang` | Rust auto-detection from `Cargo.toml`; emits cheat-sheet to stdout. Skills depend on this; cheap, standalone, language-only. Promotes `lang_detect_rust.md`, `lang_per_file_python.md`, `lang_unsupported.md`. |
| **12** | Skill bundle infrastructure + authoring skill manifest | Embedded-resource pipeline for skill manifests; file-extraction helpers; AGENTS.md section-injection helpers (Codex / OpenCode). Ships **only the authoring skill manifest** — mining / neural-verify / review skill manifests are added in their consuming slices (24 / 23 / 27). Each agent's manifest format is whatever that agent expects (Claude Code SKILL.md, Cursor `.mdc`, Antigravity `.md`, AGENTS.md section); we don't roll our own protocol — we generate per-agent. |
| **13** | `aristo install-skills` + `aristo uninstall-skills` | Per-agent install paths; `--list-agents`, `--update`, `--user` scopes. Promotes all 10 `install_skills_*.md` + `uninstall_skills_*.md`. |

⭐ **DOGFOOD ACTIVATION** (after slice 13). We run `aristo install-skills --agent=claude-code --user` against this dev environment. Claude Code uses the authoring skill to write annotations on Aristo source as we develop. Backfill commit lands here: sweep slices 1–5 schema crates, annotate via the skill.

## Milestone C — Index pipeline → v0.0.4

Goal: full inspection loop. We can see the annotations we've been writing.

| Slice | Deliverable | Notes |
|---|---|---|
| **14** | Source walker + hashing utilities (in `aristo-core`) | `syn`-based file walker for all four annotation surfaces; deterministic `text_hash` / `body_hash` per the docs; ID generation (snake_case from text + `aret_<random>` fallback). Library-only. |
| **15** | Cycle detection (in `aristo-core`) | DFS over parent graph; self-cycle special case; diamond pattern allowed; diagnostic-friendly error type (path + per-node `file:line`). |
| **16** | `aristo index` | Walks via slice 14, hashes, runs cycle detection, atomic write to `.aristo/index.toml`. `--all` ignores mtime cache. Promotes `index_standalone.md`. |
| **17** | `aristo stamp` (offline subset) | Index + ID assignment + body-drift detection + cycle detection. **Excludes B5b classification** (server-issued certificates not in scope). `--check` CI mode. Stamp and index stay separate commands per design — the separation matters once server-side B5b classification lands in stamp. Promotes `stamp_cycle_diagnostics.md`, `edit_then_stamp_surfaces_drift.md`, the stamp portions of `lifecycle_init_to_first_verify.md`. |
| **18** | `aristo show` + `aristo list` | Selector parsing for `show` (id / `fn name` / `file:line` / `mod name` / `struct name` / etc.); J2 `--filter` parsing for `list`; `--json` / `--toml` output mode; did-you-mean fallback; stale-index warning when applicable. Promotes all `show_*.md`, `list_*.md`. |

⭐ **FULL DOGFOOD LOOP** (after slice 18). Author with skill → compile (with `aristo_check`) → index → show / list. Self-annotation becomes a useful test fixture: "Aristo's index of itself parses + validates."

## Milestone D — Daily-loop health → v0.0.5

Goal: the daily authoring loop becomes safe — drift surfaces, lint enforces, hooks gate.

| Slice | Deliverable | Notes |
|---|---|---|
| **19** | Freshness preflight + `aristo status` | Shared internal preflight (per-file source mtime vs index) used by 8 reader commands. `aristo status` is the simplest first integration. Promotes `status_full_output.md`, `stale_index_preflight.md`, `stale_preflight_on_list.md`. |
| **20** | `aristo lint` | Built-in rules per mockup 07: `empty_text`, `text_too_long`, `weasel_words`, whitespace, anti-pattern phrases. `--check` (read-only, non-zero on findings) and `--fix` (autofixable rules; restages files). `--strict` also fails on `warn`. Promotes `lint_check_fail.md`, `lint_fix_restages.md`. |
| **21** | Pre-commit hook implementation | `aristo init` writes the real bash hook (Linux/macOS only — Windows users get a docs note for v0.1.0). Hook runs `aristo stamp` + `aristo lint --check` per `[lint] pre_commit` default. Un-`#[ignore]`s `tests/pre_commit_hook.rs`. Aristo's own `.git/hooks/pre-commit` now runs Aristo on Aristo. |

## Milestone E — Verify pipeline (neural only) → v0.0.6

Goal: end-to-end verification on the free tier for `verify="neural"` annotations. Heavy dogfooding — slice 23 ran against Aristo's own annotations as a ground-truth eval set, surfaced 11 lifecycle gaps post-shipment, and led to a follow-up hardening milestone that closed 8 of them.

Originally five slices (22–26). Slices 24, 25, 26 (`verify="test"` and `verify="full"`) are deferred to post-MVP — see "Deferred — post-MVP (with design blockers)" below. Milestone E now ships with just 22 + 23 and tags as v0.0.6.

| Slice | Deliverable | Notes |
|---|---|---|
| **22** | `aristo verify` dispatcher + `verify=false` path | Top-level command, J2 `--filter` reuse from slice 18, `--rerun` flag, `--check` / `--strict`, per-entry pipeline skeleton, `vlvl=false → noop` arm. Promotes `verify_false_skipped.md`, `verify_filter_rerun.md`. |
| **23** | `verify="neural"` free path | Bundles `aristo-neural-verify` skill manifest; invokes it via host agent's standard skill mechanism; status-only update. Promotes `verify_neural_free.md`. **Post-shipment hardening** (2026-05-17) closed 8 lifecycle gaps in a follow-up arc: validator-fills-hashes, Status::Inconclusive variant, validator-at-list-time skip logic, suggestion-vs-index check, strict text-drift policy, attempts persistence, stamp cascade-on-removal, loud Counterexample warning. |

## Milestone F — Critique + Review-sessions → v0.0.7

| Slice | Deliverable | Notes |
|---|---|---|
| **27** | `aristo critique` v0 | ✅ **Shipped 2026-05-17 (v0.0.7).** Renamed from `aristo review`. Bundles `aristo-critique` skill; queue-based dispatch + submit-gate + apply-pass on shared `pipeline/` infrastructure. Categorized findings (`rephrasing` / `parent-shape` / `vocabulary` / `scope` / `clarity`, severity-tagged). Default scope is **filter-required** per `docs/decisions/critique-and-pipeline-architecture.md` §D6. The two pending scenarios `critique_filter.md` + `stale_preflight_on_critique.md` describe a synchronous-output UX that pre-dates the queue-pipeline architecture; they stay in `_pending/` until a later milestone re-aligns implementation. |
| **27.5** | review-session substrate | ✅ **Shipped 2026-05-17 (v0.0.7, 10 steps).** Generic stateful triage with 4 buckets (open/accepted/rejected/pending). `Session` / `SessionKind` trait / `CritiqueReviewSession` + `ProofReviewSession` impls. Three-layer enforcement: SDK pre-check (Layer 1) + UserPromptSubmit hook (Layer 2) + skill body discipline (Layer 3). `aristo session start/active/status/decide/exit/abort/list` CLI surface. All of `.aristo/sessions/` gitignored. See `docs/decisions/review-sessions.md`. |
| **27.7** | critique v1 polish | ✅ **Shipped 2026-05-18 (v0.0.7, 8 commits).** Disposition-aware `--apply-findings` (default filters to open findings; `--include-closed` for the full view); index cache fields `last_critiqued_at_text_hash` + `last_critique_finding_count` + dispatcher skip-on-cache-hit + `--rerun` flag; `--staged` (intersects with explicit `--filter`); `--all` + `--yes` cost-gate; J2 line-range filter syntax `file=<path>:<LO>-<HI>` (consumed by critique; list / verify ignore the range). |

## Milestone G — Doc + graph → v0.0.8

| Slice | Deliverable | Notes |
|---|---|---|
| **28** | `aristo doc` | ✅ **Shipped via session B (5 sub-commits).** Per-annotation markdown to `.aristo/doc/<id-safe>.md` (`<id-safe>` replaces `:` with `__`); incremental writes; `--include-status` (B5b status bake-in); `--summary` (project-level `_summary.md`); `--check` CI gate. Promoted 5 of 6 `doc_*.md`. |
| **29** | `aristo graph` | ✅ **Shipped 2026-05-18 (11 commits + v0.0.8 tag).** Mermaid (default stdout); DOT; SVG (via `dot` subprocess; friendly missing-binary error with three-platform install hints + two no-Graphviz alternatives); J2 `--filter` (id/file/parent/status, with line-range syntax `file=<path>:<LO>-<HI>` from slice 27.7); `--exclude-assumes`, `--include-orphans`, `--include-status`, `--depth`, `--out`, `--format`. Visual encoding per the mockup: shape distinguishes intent (rectangle) vs assume (hexagon); color encodes verify level OR B5b status (via `--include-status`); red border on critical states. Composes with `aristo doc --include-graph` (slice 29 commit 10). Promoted 7 of 12 `graph_*.md` scenarios; 5 stay in `_pending/` with documented reasons (SVG byte-mismatch across Graphviz versions covered imperatively by `tests/graph_svg.rs`; subtree / direct-children semantics need rewriting against shared fixture; lifecycle composite uses SVG-embedded graph which conflicts with the chosen Mermaid-embedded design). See CHANGELOG `[v0.0.8]` for the full per-commit notes. |
| **30** | `aristo_doc` cargo feature in `aristo-macros` | ✅ **Shipped via session B (commit `73c95c7`).** Proc-macro injects `#[doc = include_str!("...")]` from `.aristo/doc/` when feature enabled. Off by default; explicit `id = "..."` required when feature on (the macro can't predict stamp's id assignment). Cargo-fixture imperative tests in `tests/aristo_doc_imperative.rs`. |

## Milestone H — MVP → v0.1.0

Goal: feature-complete MVP. After this milestone the SDK is shippable as the first preview release on crates.io.

**Scope adjustment 2026-05-18:** slice 33 (`aristo verify --audit-only`) deferred to Phase 2. Rationale: the offline shell would return all-zeros for every crate in existence today (no `verified_outcome` entries can be issued without server-side cert signing). The "ship a shell for forward-compat" argument is weak — any consumer CI gating on it would also be waiting for Phase 2. Punting alongside the real cert-validation behavior gives a coherent feature rather than a half-shipped surface.

| Slice | Deliverable | Notes |
|---|---|---|
| **31** | `aristo badge` | ✅ **Shipped via session B (commit `d917cd5`).** Reads index, computes metrics (`aristos-count`, `verification-rate`); SVG output; `--out` or stdout; `--style={flat,flat-square,for-the-badge}`. **Offline-only — `--strict` is server-side and remains deferred to Phase 2.** |
| **31.5** | tiered badge | ✅ **Shipped via session B (commit `c3971b9` and `2b3edf9`).** `aristo badge --metric={count,rate,tier}` defaults to `tier`. Adds `aristo_core::badge::compute_tier` (D7 score formula + D8 cutoffs + D4 Areté gate) and `aristo_core::walk::count_fns_per_module` (the coverage denominator walker, excludes `#[cfg(test)]` recursively, counts trait defaults). SVG picks up the D11 per-tier palette (#8a8378 / #c9a87c / #C0362C / #8c2913 / #d4a017) and embeds the locked bridge-as-Ω logo in every badge. Promoted `badge_tier_default.md`. |
| **32** | `aristo rename` | ✅ **Shipped 2026-05-18.** Atomic coordinated rename across source files + `.aristo/index.toml` + `.aristo/critiques/<id>.critique` + `.aristo/proofs/<id>.proof`. `--dry-run` plans without writing. Validation: target-collision rejection (with `<file>:<site>` hint), readable→opaque-prefix rejection (F1-b), `aristos:` in either direction rejected with the "deferred until Phase 2 sync ships" message (scope trim per HANDOFF-SLICE-32.md). F1-c opaque (`aret_*`) → readable promotion supported with the canonical promotion note. Apply order is source→artifacts→index LAST so partial failure is recoverable via `aristo stamp` (no real transactional rollback — best-effort recoverable). Span-based byte-substitution via `aristo_core::walk::scan_id_occurrences` preserves whitespace + comments verbatim. Three `rename_*.md` scenarios promoted (dry-run + actual run + error cases + opaque promotion). 7 commits, ~1050 LOC. |
| ~~33~~ | ~~`aristo verify --audit-only`~~ | **Deferred to Phase 2.** Without server-side cert signing, no crate has `verified_outcome` entries that aren't local; the offline shell would return all-zeros for every project. Ships alongside the real cert-validation behavior in Phase 2. The two pending scenarios `verify_audit_only.md` + `verify_audit_only_check.md` stay in `_pending/` until then. |
| **34** | CI gate composite scenario | Promotes `lifecycle_ci_gates.md`. Mostly integration verification — the per-command `--check` modes wired uniformly across slices 17/20/22/28 should already compose; this slice asserts that they do, end-to-end. |
| **35** | v0.1.0 release prep | README polish; crates.io metadata (description, keywords, categories, repository, license, documentation links); `cargo publish --dry-run` for each crate (`aristo-core`, `aristo-macros`, `aristo-cli`, `aristo`); workspace version bump; `git tag v0.1.0`. |

**MVP path from here**: slice 34 → 35 (≈ 3-6 commits, ~1-2 focused days).

---

## Deferred — post-MVP (with design blockers)

These slices were originally scoped into milestone E but are deferred until the prerequisite design work settles. Background, research, surveyed options, and the open questions that must be answered before implementation can start: **`docs/deferred/verify-test-design.md`**.

| Slice | Deliverable | Blocker |
|---|---|---|
| ~~24~~ | `verify="test"` free path | Specification schema and injection mechanism are coupled; four open design questions must be answered before implementation (macro behavior on missing spec, injection-point enum scope, mining-skill failure surface, anchor flexibility). |
| ~~25~~ | `aristo_verify` cargo feature in `aristo-macros` | Blocked on 24 (this is the injection half). |
| ~~26~~ | J4 free-tier `verify="full"` downgrade | Blocked on 24 (routes to the test path). |

When this work is picked back up, the steps in order are: settle the open questions → write the concepts doc (`annotations vs specifications` vocabulary lock) → write the lifecycle diagram (mermaid viewer section 5) → promote a new slice into this roadmap → implement.

## Decided (recorded for posterity)

These were the open questions at roadmap-design time. Settled answers:

1. **Slice grain.** ~30 small slices, one observable command/feature per slice. Matches the slice 1–5 precedent.
2. **Skill invocation.** Whatever each host agent's standard skill mechanism is — Claude Code SKILL.md, Cursor rules, Antigravity skills, AGENTS.md sections for Codex/OpenCode. We generate per-agent manifests; we don't roll our own protocol.
3. **MVP cutoff.** End of milestone H = v0.1.0. Feature-complete + shipped to crates.io as the first preview.
4. **`aristo init` and `Cargo.toml`.** Default behavior: print the `aristo = "..."` dependency line for the user to copy in. `-f` / `--force` actually modifies `Cargo.toml`.
5. **Pre-commit hook.** Bash only. Windows users get a docs note for v0.1.0; cross-platform hook is post-MVP.
6. **`aristo stamp` vs `aristo index`.** Keep both. The separation matters once server-side B5b classification lands in stamp (Phase 2).
7. **`aristo verify --audit-only`.** ~~Ship the offline shell in milestone H (slice 33) so downstream consumers' CI doesn't break on free-tier crates.~~ **Revised 2026-05-18: punted to Phase 2.** Without server-side cert signing, the offline shell would return all-zeros for every crate — nothing useful to ship. Bundled alongside the real cert-validation behavior in Phase 2.
8. **Skill install scope (for our own dogfood setup).** `--user` scope. Surfaces UX issues with the cross-project install path early; works in any repo we touch.
9. **Annotation density.** Aggressive. We can always relax later; we want to feel the full friction now. **Caveat:** intent is for high-level invariants and properties, **not** a replacement for normal Rust doc comments — it supplements them. (See "Annotation discipline" above.)
10. **`aristo_check` cargo feature.** Pulled forward from milestone H to milestone A (slice 8). Anything that helps us write and check intent ships as early as possible.

## Deferred — Phase 2 (server slice — separate roadmap)

- `aristo auth login` / `aristo auth logout`
- `aristo sync` (first-bind + `--rebind`)
- `aristo unbind`
- `aristo suggestions {list, apply, reject}`
- All paid-tier verify paths (`n_tier=Paid`, `t_tier=Paid`, `f_tier=Paid` per `03-verify-execution.mmd`)
- B5b classification in `aristo stamp` (verified / stale / orphan / forged / pending-deepen)
- `aristo verify --audit-only` in full (cert validation against bundled public keys + the `--check` CI gate variant; slice 33 was originally scoped for v0.1.0 as an offline shell but punted entirely on 2026-05-18 — the shell with no server-side signing produces all-zeros output and there's no value in shipping it before the cert behavior is real)
- `aristo verify --audit-only --strict` (publisher provenance via `aretta.dev/registry/`)
- `aristos:`-namespace renames in `aristo rename` (slice 32 ships free-tier rename only for bare + `aret_*` ids; the `aristos:` path is rejected with a "deferred until `aristo sync` ships" message because the server-binding rebind flow makes no sense without `sync` to call afterward)
- `aristo badge --strict`
- The `aristos:` namespace prefix on source ids (only `aristo sync` writes it)
- All 9 server-side `_pending/` scenarios catalogued in `WORKFLOW-COVERAGE.md` §2
- The `aristo verify --require <method>` flag (G3 — gates "actually require full to count")

## Deferred — Phase 3+

- Per-language SDKs: `aristo-python`, `aristo-go`, `aristo-typescript` as sibling repos per K3
- `aristo dashboard`
- `aristo pr-bot` / GitHub App integration
- `aristo suggest` (developer-side suggest queue)
- Shell completions (`aristo completions {bash,zsh,fish}`)
- Telemetry implementation (`[telemetry] enabled`)
- Corpus contribution (`[corpus] contribute`)
- Cross-platform pre-commit hook (Windows `.cmd` form)
- Sphinx / godoc / TypeDoc bridges (rustdoc-only in v0.1.0)
- Custom regex lint rules via `aristo.toml [lint.rules]` user extension (built-ins only in v0.1.0)
