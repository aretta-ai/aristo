**Aristo verified intent — `doc_output_shape_pivots_on_empty_doc_dir_not_per_run_counts`**

`aristo doc` chooses its output shape by whether `.aristo/doc/` was empty before the run, not by whether any file went unchanged this time. A first run into an empty directory emits the onboarding footer; every later run prints the compact updated/unchanged summary with no footer. A regression that pivoted on the per-run unchanged count instead would re-emit the onboarding footer on any run that happens to rewrite every file — for instance a schema upgrade that touches all of them.

<sub>Verify level: **neural**</sub>

---
