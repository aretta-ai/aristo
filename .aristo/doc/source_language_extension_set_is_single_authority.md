**Aristo verified intent — `source_language_extension_set_is_single_authority`**

The annotation walk, freshness preflight and aristo lang route their source-extension selection through this function (via the path lookup), so they agree on which files count as source. Filtering `.c`/`.h` in here but not in the freshness walk would silently stop drift detection for C files — they would be indexed once but never re-checked for staleness. `.h` is treated as C.

<sub>Verify level: **test**</sub>

---
