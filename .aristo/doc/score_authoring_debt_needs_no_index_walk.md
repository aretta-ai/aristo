**Aristo verified intent — `score_authoring_debt_needs_no_index_walk`**

Scoring the authoring-debt (agent) signal needs ONLY the edit counter — never the index-derived Metrics. The PostToolUse hook that drives it fires on every edit, so it must not walk the source tree per edit; this scores the one signal straight from the counter, reusing the registry's base and the identical `pressure * factor >= 1` fire rule so it can't drift from `score`.

<sub>Verify level: **test**</sub>

---
