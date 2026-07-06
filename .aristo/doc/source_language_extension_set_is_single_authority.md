**Aristo verified intent — `source_language_extension_set_is_single_authority`**

Both the annotation walk and the freshness preflight route their extension check through this one function, so they can never disagree on which files count as source. Filtering `.c`/`.h` in here but not in the freshness walk would silently stop drift detection for C files — they would be indexed once but never re-checked for staleness. `.h` is treated as C.

<sub>Verify level: **test**</sub>

---
