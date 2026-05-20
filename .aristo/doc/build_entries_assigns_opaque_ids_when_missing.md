**Aristo verified intent — `build_entries_assigns_opaque_ids_when_missing`**

Every discovered annotation gets an id, sourced in this order: user-written `id =`, then a snake_case slug derived from the text, then a random `aret_…` opaque id. The build never returns an entry without an id; there is no `unindexed` half-state.

<sub>Verify level: **test**</sub>

---
