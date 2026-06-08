**Aristo verified intent — `rejection_log_append_relies_on_posix_write_atomicity`**

Rejection-log writes open with `OpenOptions::append(true).create(true)` and append the serialized JSON line. No locking is needed: the file is per-workspace and gitignored, so there are no cross-process writers outside a single user session, and within a session the single-writer flow appends under `O_APPEND`, which positions each write at end-of-file. A refactor that read-modify-wrote the whole file would lose this property and need explicit locking.

<sub>Verify level: **neural**</sub>

---
