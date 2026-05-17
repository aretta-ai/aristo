# Round 3 audit — slices 19–23 backfill + slices 22/23 fresh intents

**Date:** 2026-05-16
**Scope:** 14 intents added since milestone-C reflection close. Backfilled retroactively for slices 19/20/21 + tasks 36/37 (5 intents) and concurrent-authored during slices 22/23 (9 intents).

## Results

| Verdict | Count |
|---|---|
| Pass content gate | 14 / 14 |
| Delete | 0 |
| Keep as-is | 8 |
| Rewrite for style | 5 |
| Verify-level shift (neural → test) | 4 |

Zero deletes — the strengthened concurrent-authoring workflow + backfill discipline produced higher per-intent quality than round 2.

## Verify-level shifts (neural → test)

Four intents had direct unit tests at the load-bearing site but were marked `neural` because the intent was written before (or concurrently with) the test:

1. `verify_skips_clean_entries_unless_rerun` — covered by `clean_verified_statuses_are_skipped_by_default` + `unknown_and_stale_are_not_clean_verified`
2. `validator_collects_all_failures_not_short_circuit` — covered by `collects_multiple_failures_not_short_circuit`
3. `validator_rejects_grounding_in_refuted_or_docs_only` — three rejection cases each tested
4. `proof_file_id_uses_colon_underscore_underscore` — `id_from_filename_*` tests cover the roundtrip

## Rewrites (style)

Five intents trimmed for length / removed speculation about future features:

1. `workspace_load_config_degrades_to_default` — dropped "e.g. a future `aristo config check`" parenthetical (74 → 53 words)
2. `skill_install_must_use_resolved_content` — dropped "future bundle hash" speculation (65 → 51 words)
3. `verify_false_arm_is_intentional_skip` — led with the invariant instead of the motivation (62 → 38 words)
4. `verify_skips_clean_entries_unless_rerun` — re-ordered to lead with policy, supporting cost reasoning second (61 → 44 words)
5. `proof_tree_uses_path_addressed_flat_list` — dropped "future diff tooling" speculation (73 → 50 words)

Pattern across all five: **speculation about future use cases is filler.** The load-bearing claim is the design choice today; future justifications dilute it.

## Process learning surfaced

**Default-to-neural is a systematic bias.** 9 of 14 new intents were neural. After re-audit, 4 of those should have been `test`. The cause: when intent and test land in the same commit, the intent is written *first* (concurrent-authoring discipline says "write inline as you make the decision"), at which point no test exists yet to lean on. The verify level defaults to `neural`. The test arrives later; verify level is rarely re-assessed.

**Fix added to PHILOSOPHY.md:** the verify-level re-check rule. After writing each test, re-read the most-recently-added intent at that site; if the test fires on the load-bearing claim, shift the intent's verify level to `test`. This is now part of P-VERIFY-MATCHES-SHAPE.

## What the round confirmed about the new concurrent-authoring discipline

- Zero deletes is meaningful — the content gate held on first-pass writes when applied concurrently. Round 1 had 3 deletes / 15 reviewed (20% slop rate); round 2 had 0 deletes / 10. Round 3: 0 / 14.
- The rewrite ratio (5 / 14 ≈ 36%) is comparable to round 2 (10 / 10 = 100%, though most were minor). Tighter authoring at first pass, but speculation about future tooling remains a recurring trim target.
- Refactor-trap naming (P-NAME-THE-REFACTOR-TRAP) was present in 13 / 14 intents — internalized.

## Backfilled-vs-concurrent split

Of the 14 intents, 5 were backfilled (retroactive audit of slices 19/20/21 + tasks 36/37 work that shipped without intents). 9 were concurrent-authored during slices 22/23 after the workflow strengthening. Both groups passed the gate; the backfilled group required no more rewrites than the concurrent group — suggesting the gate is the load-bearing test, not the authoring cadence. Concurrent authoring's value is preventing *missed* intents, not improving the quality of intents that get written.
