**Aristo verified intent — `atomic_write_via_tempfile_rename`**

A crash mid-write leaves either the prior file or the new file at the target — never a partial one. The temp file's suffix is fixed, not randomized, so two concurrent invocations clash on the temp file — intentional, since running two indexers against one workspace is a user error we surface loudly.

<sub>Verify level: **neural**</sub>

---
