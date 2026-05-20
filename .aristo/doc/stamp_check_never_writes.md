**Aristo verified intent — `stamp_check_never_writes`**

When `--check` is set, `aristo stamp` never writes the index. CI relies on this for drift detection: a regression that mutates the index under `--check` would silently mask the drift it was meant to catch.

<sub>Verify level: **test**</sub>

---
