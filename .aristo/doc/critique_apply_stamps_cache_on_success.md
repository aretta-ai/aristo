**Aristo verified intent — `critique_apply_stamps_cache_on_success`**

`aristo critique --apply-findings` stamps `last_critiqued_at_text_hash` + `last_critique_finding_count` on each accepted critique's IntentEntry in the same operation that validates the .critique file. The cache and the on-disk .critique MUST be updated together: a writer that stamps before validate or skips the stamp leaves readers seeing stale-cache + fresh-file divergence (the cache claims a critique is current when the file says otherwise, or vice versa). Idempotent: re-running on an unchanged .critique re-writes the same values. AssumeEntry is skipped (the cache fields are IntentEntry-only by design — assumes are documentation-only annotations per A5 and don't have the same cache lifecycle as verifiable intents).

<sub>Verify level: **neural**</sub>

---
