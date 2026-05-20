**Aristo verified intent — `doc_check_never_writes`**

`aristo doc --check` is a CI gate: it MUST NOT write to `.aristo/doc/` under any circumstance — its job is to detect drift so CI can block a merge that has stale doc artifacts. A regression that wrote during --check would silently fix the thing CI was supposed to catch.

<sub>Verify level: **neural**</sub>

---
