**Aristo verified intent — `build_entries_assigns_deterministic_ids_when_missing`**

Every discovered annotation gets an id: the user-written `id =` if present, otherwise a deterministic content-addressed `aret_…` id derived from the annotation's kind, text, and site. The build never returns an entry without an id; there is no `unindexed` half-state. Because the generated id is a pure function of identity, re-stamping unchanged source mints the same ids, so the index keeps each entry's prior status and proof instead of treating it as removed-then-new.

<sub>Verify level: **test**</sub>

---
