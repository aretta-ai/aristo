**Aristo verified intent — `review_new_vs_backlog_needs_a_baseline`**

New vs backlog is computed against the SessionStart edit-window baseline: an unreviewed intent is 'new' only when a baseline WAS captured and its id is absent from it (authored this session). With no baseline the split is suppressed (new_count = 0) rather than guessed — calling everything 'new' on a fresh checkout would misreport a long-standing backlog as this-session's work.

<sub>Verify level: **test**</sub>

---
