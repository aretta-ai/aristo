---
date: 2026-05-15
slice: 17
file: crates/aristo-cli/src/commands/stamp.rs:162
id: merge_status_preserves_when_body_unchanged
verdict: keep-rewrite
principles: [P-SPEC-STYLE]
verify_was: test
verify_is: test
---

## Original (v0)

> merge_status_from_prev preserves status when body_hash is unchanged and flips Verified/Tested/Neural to Stale when body_hash drifts — the contract that lets developers trust stamp's pre-commit signal: anything still showing Verified after stamp is verified for THIS code, not some prior version of it.

## Better (v2)

> Status after stamp reflects the current code, not any prior version. Body-unchanged entries keep their prior status. Body-drifted entries with verified-class status (Verified, Tested, Neural) flip to Stale. Other prior statuses pass through.

## Why the gap

v0 leads with a function-name reference and embeds the load-bearing user-promise mid-sentence ("the contract that lets developers trust…"). v2 elevates the user-promise to a lead sentence ("Status after stamp reflects the current code"), then enumerates the three rules cleanly. Same content, much easier to read; the promise is what callers care about, the rules are how it's kept.

## Verify level

- was: `test`
- is: `test`
- reason: each rule is directly testable. Existing tests cover all three: `stamp_preserves_status_when_body_unchanged`, `stamp_flips_verified_to_stale_on_body_change`, `stamp_flips_text_changed_body_held_preserves_status`.
