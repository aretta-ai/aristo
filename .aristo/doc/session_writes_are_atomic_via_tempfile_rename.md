**Aristo verified intent — `session_writes_are_atomic_via_tempfile_rename`**

Session writes go through `atomic_write` (temp-file + rename) so a concurrent reader cannot observe a partially-serialized session file. The session TOML is the single source of truth for in-flight state; a half-written file would deserialize-error and look like 'session is gone' from the reader's perspective. A refactor that used `fs::write` directly would re-introduce the partial-read window between open and close.

<sub>Verify level: **neural**</sub>

---
