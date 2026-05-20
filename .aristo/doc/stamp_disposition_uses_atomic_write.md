**Aristo verified intent — `stamp_disposition_uses_atomic_write`**

Stamping a disposition mutates the on-disk .critique file via atomic_write (temp + rename). The .critique file is the audit trail of every triage decision against this critique; a non-atomic write would deserialize-fail on partial state and look to `--apply-findings` like the critique has vanished.

<sub>Verify level: **neural**</sub>

---
