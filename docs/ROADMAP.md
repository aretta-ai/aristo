# Aristo SDK — Phase 1 roadmap

Phase 1 = the offline Rust SDK MVP. Server, auth, `aristos:` bindings, and per-language SDKs are out of scope (see `WORKFLOW-COVERAGE.md` §2 and the deferred lists at the bottom of this file).

The roadmap is organized into **8 milestones (A–H)**, totalling **30 numbered slices** continuing from the slice 1–5 series already shipped. Each milestone closes with a workspace version bump and a git tag — so the project gains a real release cadence from now on instead of one big-bang publish at the end.

**MVP = end of milestone H = v0.1.0.**

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

## Milestone E — Verify pipeline → v0.0.6

Goal: end-to-end verification on the free tier. Heavy dogfooding — every new verify-path slice runs against Aristo's own annotations as a ground-truth eval set.

| Slice | Deliverable | Notes |
|---|---|---|
| **22** | `aristo verify` dispatcher + `verify=false` path | Top-level command, J2 `--filter` reuse from slice 18, `--rerun` flag, `--check` / `--strict`, per-entry pipeline skeleton, `vlvl=false → noop` arm. Promotes `verify_false_skipped.md`, `verify_filter_rerun.md`. |
| **23** | `verify="neural"` free path | Bundles `aristo-neural-verify` skill manifest; invokes it via host agent's standard skill mechanism; status-only update. Promotes `verify_neural_free.md`. |
| **24** | `verify="test"` free path (sans cargo feature) | Bundles `aristo-mine-assertions` skill manifest; invokes it; writes `.aristo/specs/<id>.spec`; parses results; updates status. Skips actual cargo-test injection (slice 25 closes that loop). Promotes `verify_test_free_full_pipeline.md`, `verify_default_skips_clean_entries.md`, `verify_rerun_keeps_clean_entries.md`. |
| **25** | `aristo_verify` cargo feature in `aristo-macros` | Proc-macro reads `.aristo/specs/<id>.spec` and injects assertions into the function body when feature enabled. Cargo-fixture imperative test. Closes the verify=test loop end-to-end. |
| **26** | J4 free-tier `verify="full"` downgrade | One-line note + routes to test path. Source `verify="full"` preserved unchanged for the day the user upgrades. Promotes `verify_free_tier_downgrade.md`. |

## Milestone F — Review → v0.0.7

| Slice | Deliverable | Notes |
|---|---|---|
| **27** | `aristo review` | Bundles `aristo-review-skill` manifest; invokes it; parses categorized findings (`rephrasing` / `parent-shape` / `vocabulary` / etc., severity-tagged); J2 `--filter` with line-range form; lint-blocked-target skip with pointer to `aristo lint --fix`; updates index `last_reviewed_at_text_hash` for caching. Promotes `review_filter.md`, `stale_preflight_on_review.md`. We review our own annotations with this — findings inform the next round of authoring-skill polish. |

## Milestone G — Doc + graph → v0.0.8

| Slice | Deliverable | Notes |
|---|---|---|
| **28** | `aristo doc` | Per-annotation markdown to `.aristo/doc/<id-safe>.md` (`<id-safe>` replaces `:` with `__`); incremental writes; `--include-status` (B5b status bake-in); `--summary` (project-level `_summary.md`); `--check` CI gate. Promotes 5 of 6 `doc_*.md`. |
| **29** | `aristo graph` | Mermaid (default stdout); DOT; SVG (via `dot` binary; friendly missing-binary error with platform install hints); J2 `--filter`; `--exclude-assumes`; `--include-status`; `--depth`; `--include-orphans`; `--out`. Promotes all 11 `graph_*.md`. The `--include-graph` from doc lands here too via slice composition. |
| **30** | `aristo_doc` cargo feature in `aristo-macros` | Proc-macro injects `#[doc = include_str!("...")]` from `.aristo/doc/` when feature enabled. Cargo-fixture imperative test. |

## Milestone H — Auxiliary + audit → v0.1.0 (MVP)

Goal: feature-complete MVP. After this milestone the SDK is shippable as the first preview release on crates.io.

| Slice | Deliverable | Notes |
|---|---|---|
| **31** | `aristo badge` | Reads index, computes metrics (`aristos-count`, `verification-rate`); SVG output; `--out` or stdout; `--style={flat,flat-square,for-the-badge}`. **Offline-only — `--strict` is server-side and remains deferred to Phase 2.** Promotes `stale_preflight_on_badge.md`, badge portion of `lifecycle_ship_with_doc_and_graph.md`. |
| **32** | `aristo rename` | Atomic across src + spec + index; `--dry-run`; cross-namespace rejection (per K1); readable→opaque rejection. Warns but allows on dirty tree. Promotes all `rename_*.md`. |
| **33** | `aristo verify --audit-only` (offline shell) | Reports counts across `verified` / `stale` / `orphan` / `pending-deepen` / `forged`. On a free-tier-only project with no `verified_outcome` entries, reports all-zeros — useful so downstream consumers' CI scripts don't break when they pull a free-tier crate. Composes with `--check`. The `--strict` cross-check against `aretta.dev/registry/` remains deferred to Phase 2. Promotes `verify_audit_only.md`, `verify_audit_only_check.md`. |
| **34** | CI gate composite scenario | Promotes `lifecycle_ci_gates.md`. Mostly integration verification — the per-command `--check` modes wired uniformly across slices 17/20/22/28 should already compose; this slice asserts that they do, end-to-end. |
| **35** | v0.1.0 release prep | README polish; crates.io metadata (description, keywords, categories, repository, license, documentation links); `cargo publish --dry-run` for each crate (`aristo-core`, `aristo-macros`, `aristo-cli`, `aristo`); workspace version bump; `git tag v0.1.0`. |

---

## Decided (recorded for posterity)

These were the open questions at roadmap-design time. Settled answers:

1. **Slice grain.** ~30 small slices, one observable command/feature per slice. Matches the slice 1–5 precedent.
2. **Skill invocation.** Whatever each host agent's standard skill mechanism is — Claude Code SKILL.md, Cursor rules, Antigravity skills, AGENTS.md sections for Codex/OpenCode. We generate per-agent manifests; we don't roll our own protocol.
3. **MVP cutoff.** End of milestone H = v0.1.0. Feature-complete + shipped to crates.io as the first preview.
4. **`aristo init` and `Cargo.toml`.** Default behavior: print the `aristo = "..."` dependency line for the user to copy in. `-f` / `--force` actually modifies `Cargo.toml`.
5. **Pre-commit hook.** Bash only. Windows users get a docs note for v0.1.0; cross-platform hook is post-MVP.
6. **`aristo stamp` vs `aristo index`.** Keep both. The separation matters once server-side B5b classification lands in stamp (Phase 2).
7. **`aristo verify --audit-only`.** Ship the offline shell in milestone H (slice 33) so downstream consumers' CI doesn't break on free-tier crates. Real cert-validation behavior lands with the server slice (Phase 2).
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
- Real `aristo verify --audit-only` cert validation against bundled public keys (slice 33 ships only the offline shell)
- `aristo verify --audit-only --strict` (publisher provenance via `aretta.dev/registry/`)
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
