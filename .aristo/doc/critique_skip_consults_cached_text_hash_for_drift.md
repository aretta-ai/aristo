**Aristo verified intent — `critique_skip_consults_cached_text_hash_for_drift`**

`aristo critique` consults `last_critiqued_at_text_hash` on each IntentEntry before re-enqueueing it. If the cached value equals the entry's current `text_hash`, the existing .critique file is up to date and re-running the LLM would burn tokens for the same answer — skip the enqueue. This is the whole point of the cache: a refactor that always re-enqueues regardless of cache state re-introduces the daily-loop LLM cost the cache exists to amortize. AssumeEntries and entries with no cache (the field is `None`) are NEVER skipped — for assumes the cache is irrelevant; for first-time entries the absent cache means "no critique on record" so the dispatcher must produce one. `--rerun` bypasses the cache entirely.

<sub>Verify level: **neural**</sub>

---
