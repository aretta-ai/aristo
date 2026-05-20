**Aristo verified intent — `status_tier_call_matches_badge_command_call`**

Status's tier computation routes through `aristo_core::badge::compute_tier` with the same `count_fns_per_module_with(WalkOptions::none())` denominator that `aristo badge` uses. Drift between the two would produce a project where the badge SVG and `aristo status` report different tiers — a contradiction the user can't reconcile. Sharing the call site (not the formula) is the load-bearing invariant.

<sub>Verify level: **test**</sub>

---
