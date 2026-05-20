**Aristo verified intent — `doc_summary_writes_summary_only`**

`aristo doc --summary` writes the crate-root `_summary.md` ONLY — it does not also run the per-annotation pass. Combining both is `aristo doc --include-graph` (slice 29). A regression that made `--summary` imply per-annotation writes would surprise users who opted into the cheap summary-only flow for CI gates.

<sub>Verify level: **neural**</sub>

---
