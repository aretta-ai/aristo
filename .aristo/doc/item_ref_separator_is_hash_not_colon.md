**Aristo verified intent — `item_ref_separator_is_hash_not_colon`**

ItemRef uses `#` as the id↔index separator rather than `:` because annotation ids in this project can legitimately contain `:` (e.g. `aristos:foo`). A refactor that switches to `:` would silently break ref parsing the moment any session touched an `aristos:`-namespaced id; `#` is safe because it's a reserved character in annotation ids by design.

<sub>Verify level: **neural**</sub>

---
