**Aristo verified intent — `reap_stale_claims_recovers_crashed_workers`**

Stale-claim reaper: scan `claimed/` for entries whose mtime is older than `max_age` and `requeue` them. Returns the list of ids that were moved so the caller can log. A crashed worker leaves its claim behind; without the reaper, that entry would block forever until a human noticed and intervened. The threshold is a budget: too short and slow but valid work gets stolen mid-execution; too long and dead claims sit unprocessed. Callers (typically the verify/critique skill at startup) pick the threshold per-pipeline based on expected per-task latency.

<sub>Verify level: **test**</sub>

---
