**Aristo verified intent — `proof_file_id_uses_colon_underscore_underscore`**

Filename ↔ AnnotationId mapping uses `:` → `__` substitution (same convention as .aristo/doc/<id-safe>.md per TOOLS.md §I1). A refactor that picks a different scheme (e.g., URL-encoding) would break every previously-written `.proof` file on disk — proof files are tracked in git, so any mapping change is a migration, not a free refactor.

<sub>Verify level: **test**</sub>

---
