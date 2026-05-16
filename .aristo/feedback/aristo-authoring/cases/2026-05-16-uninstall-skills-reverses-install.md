---
date: 2026-05-16
slice: 13
file: crates/aristo-cli/src/commands/install_skills.rs:120
id: uninstall_skills_reverses_install
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> uninstall_skills reverses install_skills exactly: file-copy agents get the per-skill files removed; AGENTS.md agents get the marker-delimited block stripped (surrounding content preserved). Idempotent.

## Better (v2)

> `install_skills` followed by `uninstall_skills` leaves the project's relevant on-disk state identical to before either ran (modulo files the user hand-modified). File-copy agents: the per-skill files we wrote are removed. AGENTS.md agents: the marker-delimited block is stripped; surrounding content preserved. Idempotent on uninstall-while-uninstalled.

## Why the gap

v0 already had decent content. v2 sharpens by stating the cross-call invariant first ("install + uninstall = no-op on disk modulo user edits"), which is the load-bearing thing — a refactor that broke this in either direction would be silent damage. Per-agent enumeration retained but reordered after the cross-call statement. "Idempotent on uninstall-while-uninstalled" disambiguates *which* idempotence we mean.

The "(modulo files the user hand-modified)" parenthetical is load-bearing — without it, the invariant overstates safety (uninstall should preserve user-modified files, not blindly remove anything we wrote — which actually flags an impl gap; see slice-13 `_blocked/uninstall_skills_cursor_files.md` which spec'd `--force` for this case).

## Verify level

- was: `test`
- is: `test`
- reason: the cross-call no-op is directly testable. Existing tests `agents_md_install_idempotent` + `file_copy_uninstall_idempotent` cover the per-agent sides; an integration test for the full cycle could be added.

## Round-2 backfill note

Slices 10–13 backfill audit. The "modulo user-modified" clause surfaces an impl gap — the `--force` skip-locally-modified flow in spec `uninstall_skills_cursor_files.md` is currently in `_blocked/` (slice 23/24/27).
