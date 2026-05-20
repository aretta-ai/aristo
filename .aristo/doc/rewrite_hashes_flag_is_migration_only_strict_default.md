**Aristo verified intent — `rewrite_hashes_flag_is_migration_only_strict_default`**

Migration-only `--rewrite-hashes` clears every stored ground hash before validation so the staleness check is skipped, then the post-accept stamping pass repopulates them from current source. Without this flag, stamped hashes act as freshness anchors and mismatches are rejected as staleness — the strict default. The flag is documented as migration-only to discourage routine use; routine `--apply-verdicts` relies on the staleness check.

<sub>Verify level: **test**</sub>

---
