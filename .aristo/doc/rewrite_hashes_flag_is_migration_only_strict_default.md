**Aristo verified intent — `rewrite_hashes_flag_is_migration_only_strict_default`**

Under migration-only `--rewrite-hashes`, this nulls every stored ground hash before validation, so the staleness check has no anchor to compare against and is skipped. Dropping the freshness anchors is the deliberate migration mechanism, not a bug: it is the only path that clears them, and it runs only when the operator opts in via the flag.

<sub>Verify level: **test**</sub>

---
