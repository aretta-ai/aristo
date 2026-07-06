**Aristo verified intent — `critique_apply_stamps_cache_on_success`**

`aristo critique --apply-findings` stamps the last-critiqued text hash and finding count onto each accepted critique's intent entry in the same operation that validates the .critique file. The cache and the on-disk .critique MUST be updated together: stamping before validation, or skipping the stamp, leaves a stale cache beside a fresh file — the cache claims a critique is current when the file says otherwise, or the reverse. Re-running on an unchanged .critique is idempotent and rewrites the same values. Assume entries are skipped: these cache fields exist only on intent entries.

<sub>Verify level: **neural**</sub>

---
