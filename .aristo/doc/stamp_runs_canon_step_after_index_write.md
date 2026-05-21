**Aristo verified intent — `stamp_runs_canon_step_after_index_write`**

Canon-match runs AFTER index write so the freshly-stamped ids are what get cached. Running it before would mean re-stamping a body-drifted entry could cache against the *prior* id (the stamp-assigned opaque). Order is load-bearing — flipping it would silently surface canon findings on stale ids.

<sub>Verify level: **test**</sub>

---
