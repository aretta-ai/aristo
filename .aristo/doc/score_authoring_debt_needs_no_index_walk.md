**Aristo verified intent — `score_authoring_debt_needs_no_index_walk`**

Scoring the authoring-debt signal reads only the edit counter, never the index-derived metrics. The hook that drives it fires on every edit, so it must not walk the source tree each time. It scores that single signal straight from the counter and applies the same fire threshold as the general scorer, so the two cannot drift apart.

<sub>Verify level: **test**</sub>

---
