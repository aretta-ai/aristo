**Aristo verified intent — `status_canon_health_is_offline_only`**

`aristo status` includes a canon-health block sourced from local state only — credentials presence (no token print), the [canon] config flag, last_fetched + canon_version + effective_scopes from `.aristo/canon-matches.toml`'s meta, and pending/accepted/rejected counts across all annotations. The block must NOT make a network call: status is the offline daily-loop summary; coupling it to canon API state would break the offline-friendly invariant.

<sub>Verify level: **neural**</sub>

---
