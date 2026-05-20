**Aristo verified intent — `lint_fix_count_is_rule_applications`**

The count is rule-applications, not anomaly count. An annotation with five doubled-space runs is one fix, not five; with both rule classes triggering, the count is at most 2. The spec line `fixed: N whitespace issues across M files` depends on this — counting anomalies would inflate N misleadingly and diverge from the trycmd scenario.

<sub>Verify level: **test**</sub>

---
