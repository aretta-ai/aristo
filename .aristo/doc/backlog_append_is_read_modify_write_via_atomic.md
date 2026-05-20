**Aristo verified intent — `backlog_append_is_read_modify_write_via_atomic`**

Appending to the backlog is read-modify-write: read the existing file, push the new entry, atomic-write the result. The `pending` bucket NEVER silently drops — every deferred item must land in this file. A refactor that used `OpenOptions::append` would lose atomicity (the backlog is TOML, not line-delimited; an interrupted append produces a non-parseable file).

<sub>Verify level: **neural**</sub>

---
