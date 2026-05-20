**Aristo verified intent — `queue_dir_is_per_pipeline_namespace`**

Each pipeline's queue lives at `.aristo/<pipeline>-queue/` — a sibling directory per pipeline name. Verify and critique get distinct subdirectories; a worker for one pipeline cannot accidentally claim a task from the other. A refactor that consolidated to a single shared queue would lose this isolation; per-pipeline workers would need additional tagging at every pop site, and a mis-tag would dispatch the wrong validator at submit time.

<sub>Verify level: **test**</sub>

---
