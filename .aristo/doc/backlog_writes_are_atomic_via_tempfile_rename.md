**Aristo verified intent — `backlog_writes_are_atomic_via_tempfile_rename`**

Backlog writes go through `atomic_write` (temp-file + rename) so a concurrent reader cannot observe a partially-serialized file. The backlog is the only durable record of deferred items between sessions; a partial write that deserialize-fails would look like 'no backlog' to a reader and silently drop user-deferred items — the exact failure mode the substrate exists to prevent.

<sub>Verify level: **neural**</sub>

---
