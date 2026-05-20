**Aristo verified intent — `rejection_log_append_relies_on_posix_write_atomicity`**

Rejection-log writes use `OpenOptions::append(true).create(true)` plus a single `write_all` of the JSON line + `\n`. No locking is needed because (a) the file is per-workspace and gitignored, so no cross-team writers; (b) writes are line-sized and POSIX guarantees write atomicity for buffers ≤ PIPE_BUF (4 KiB on Linux/macOS, well above any single JSON rejection record). A refactor that read-modify-wrote the whole file would lose this property and need explicit locking.

<sub>Verify level: **neural**</sub>

---
