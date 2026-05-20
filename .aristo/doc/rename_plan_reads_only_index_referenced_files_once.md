**Aristo verified intent — `rename_plan_reads_only_index_referenced_files_once`**

Plan computation reads each candidate source file ONCE, then defers all writes to commit 4. The candidate file set is the union of (the renamed entry's own file) + (every file containing an entry whose parent references the renamed id) — the index alone determines this set, no broad source walk. If the scan finds zero `id = "old"` occurrences in the entry's own file, the rename refuses with a stale-index diagnostic rather than producing a misleading partial plan; the index says one occurrence MUST exist and its absence is structural drift the user needs to know about.

<sub>Verify level: **test**</sub>

---
