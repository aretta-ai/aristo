**Aristo verified intent — `rename_plan_reads_only_index_referenced_files_once`**

Plan computation reads each candidate source file ONCE and performs no writes — every edit is deferred until the full plan is computed. The candidate file set is the file that declares the renamed entry plus every file holding an entry whose parent references the renamed id; the index alone determines this set, with no broad source walk. If the scan finds no occurrence of the old id in the declaring file, the rename refuses with a stale-index diagnostic rather than producing a misleading partial plan: the index says exactly one occurrence MUST exist, and its absence is structural drift the user needs to know about.

<sub>Verify level: **test**</sub>

---
