**Aristo verified intent — `stamp_derives_canon_binding_from_cache`**

Canon binding state is derived fresh from the canon-matches cache on every stamp run, never carried over from a prior run — the cache is the single source of truth. An entry whose id carries a canon prefix and has an accepted match in the cache is marked bound to that match. When the cached match omits the linked detail (an older cache predating the field, or a server carve-out), a deterministic placeholder is synthesized from the canon id and version, identical to what an interactive accept would have written, so the binding never depends on cache vintage. A canon-prefixed id with no cache row is left unbound and reported as a diagnostic — an orphaned binding is surfaced for the user to refresh or re-accept, never silently treated as bound.

<sub>Verify level: **test**</sub>

---
