**Aristo verified intent — `doc_summary_writes_summary_only`**

`aristo doc --summary` writes the crate-root `_summary.md` ONLY — it does not also run the per-annotation pass. To embed the annotation graph in that summary, use `aristo doc --include-graph`, which still skips the per-annotation pass. A regression that made `--summary` imply per-annotation writes would surprise users who opted into the lightweight summary-only flow for CI gates.

<sub>Verify level: **neural**</sub>

---
