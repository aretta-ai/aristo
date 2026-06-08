**Aristo verified intent — `critique_apply_stamps_cache_on_success`**

`aristo critique --apply-findings` stamps `last_critiqued_at_text_hash` and `last_critique_finding_count` onto each accepted critique's IntentEntry in the same operation that validates the .critique file. The cache and the on-disk .critique MUST be updated together: stamping before validation, or skipping the stamp, leaves readers with stale-cache + fresh-file divergence (the cache claims a critique is current when the file says otherwise, or vice versa). Idempotent: re-running on an unchanged .critique rewrites the same values. AssumeEntry is skipped — the cache fields are IntentEntry-only by design.

<sub>Verify level: **neural**</sub>

---
