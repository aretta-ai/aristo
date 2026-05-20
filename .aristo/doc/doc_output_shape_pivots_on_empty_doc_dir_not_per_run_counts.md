**Aristo verified intent — `doc_output_shape_pivots_on_empty_doc_dir_not_per_run_counts`**

`aristo doc` output shape differs by first-run-vs-incremental: first run (empty .aristo/doc/) prints per-file `• Wrote:` lines, a `(N files written, 0 unchanged)` count, and the `Next steps` onboarding footer; subsequent runs (any pre-existing file) print `• Updated:`/`• Unchanged:` lines and a compressed `ok: doc artifacts updated. (M written, N unchanged)` summary with no onboarding footer. The pivot is whether the doc dir was empty before the run, not whether any file was unchanged this time — a regression that switched to the count-based check would emit onboarding footers on every run that happens to write all files (e.g. a schema upgrade that touches every MD).

<sub>Verify level: **neural**</sub>

---
