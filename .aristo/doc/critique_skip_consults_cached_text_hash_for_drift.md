**Aristo verified intent — `critique_skip_consults_cached_text_hash_for_drift`**

`aristo critique` skips re-enqueueing an intent whose last-critiqued text hash still equals its current text hash: the existing .critique file is current, and re-running the LLM would spend tokens for the same answer. A refactor that always re-enqueues regardless of cache state re-introduces the daily-loop LLM cost the cache exists to amortize. Two cases are never skipped: assumes (the cache does not apply) and intents with no cached hash (no critique on record yet, so one must be produced). `--rerun` bypasses the cache entirely.

<sub>Verify level: **neural**</sub>

---
