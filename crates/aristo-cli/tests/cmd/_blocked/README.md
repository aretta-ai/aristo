# `_blocked/` — spec scenarios with an explicit, time-bound implementation gap

Per CLAUDE.md §12, scenarios in this directory are **THE SPEC** (byte-for-byte). They describe behavior that is **expected and approved to ship**, but whose implementation has been scheduled for a future slice. The `cli_scenarios` test runner does **not** pick up `_blocked/`, so these don't break the build today.

## Why this directory exists (and not just `_pending/`)

- `_pending/` — spec for features whose implementation has **not been scoped** yet (Phase 2 server commands, Phase 2 paid-tier verification, etc.). No slice number, no commitment.
- `_blocked/` — spec for features whose implementation **is committed to a specific upcoming slice in the roadmap**. The scenario stays here until that slice ships, then it moves to `active/` byte-for-byte (and MUST pass).
- `active/` — runs in CI. Must pass on every commit.

The split matters because `_pending/` is the "we'll get to it eventually" pile and `_blocked/` is the "we have a concrete delivery target for this." A scenario should not loiter in `_blocked/` indefinitely — if its target slice slips, surface the slip explicitly, don't quietly extend.

## Currently blocked scenarios

| Scenario | Blocked on slice | What's missing |
|---|---|---|
| `init_creates_index_file.md` | **slice 21** (pre-commit hook implementation) | Spec demands `ok: installed pre-commit hook (.git/hooks/pre-commit)` line in `aristo init` output; current impl emits a placeholder hook only inside a git repo (slice 21 wires the real install). Spec also has a read-commands block (`aristo list` + `aristo status` after init) that worked in slice 19, but trycmd's whole-scenario semantics couple it to section 1's pre-commit-hook output. |
| `index_standalone.md` | **slice 17+** (per-file mtime cache; currently un-scoped to a specific slice) | Spec demands incremental-cache output (`47 files scanned, 3 changed since last run`, `incremental: 3 files re-walked, 44 from cache`). Slice 16 ships full-walk only; `--all` is a no-op pending the cache. Spec also demands a cycle-detection block with per-link enumeration that slice 15 cycle output doesn't yet produce. |
| `install_skills_antigravity.md` | **slices 23, 24, 27** (consuming slices for `aristo-neural-verify`, `aristo-mine-assertions`, `aristo-review-skill` bundles respectively) | Spec demands 4 skills installed; slice 13 ships only `aristo-authoring`. The other 3 skills' manifests land in their consuming feature slices per ROADMAP.md. |
| `install_skills_claude_code.md` | **slices 23, 24, 27** | Same — 4 skills demanded, 1 shipped. |
| `install_skills_cursor.md` | **slices 23, 24, 27** | Same. |
| `install_skills_codex_agents_md.md` | **slices 23, 24, 27** | Same; also demands a heredoc display of the injected marker block showing all 4 skills' content. |
| `install_skills_opencode_shares_block.md` | **slices 23, 24, 27** | Same; also demands the "reusing block already established by codex" path (depends on codex install having run first). |
| `uninstall_skills_codex_section.md` | **slices 23, 24, 27** (must have something to uninstall) | Spec demands "Removing Aristo skills for Codex" wording + "Stripped marker-delimited Aristo section" detail line. Current impl says "nothing to do" when no Aristo block is present, which is the default state of a fresh sandbox; the scenario implicitly assumes a prior install with all 4 skills. |
| `uninstall_skills_cursor_files.md` | **slices 23, 24, 27** | Spec demands "3 skills removed, 1 skipped (locally modified; pass --force)" — implies the 4-skill bundle was installed first, then one was hand-edited. |

## Promotion protocol (when a blocking slice ships)

1. The slice that closes the blocker (e.g. slice 21 for pre-commit hook) moves the matching scenario from `_blocked/` to `active/` in the SAME commit that lands the implementation.
2. The promotion is byte-for-byte — no edits to the spec (per §12).
3. The slice's CHANGELOG entry references the promoted scenario explicitly.
4. If the slice ships but the scenario still doesn't pass byte-for-byte, that's a §12 violation — fix the impl, don't edit the scenario.

## What does NOT belong here

- Scenarios whose feature is intentionally Phase 2 — those go to `_pending/`.
- Scenarios that fail because of a typo / cosmetic gap in the impl — fix the impl now.
- Scenarios I rewrote to fit a Phase-1 subset — restore the original spec; if it's blocked, move to `_blocked/` with the right slice mapping above.
