**Aristo verified intent — `deterministic_id_is_pure_function_of_identity`**

A stamp-assigned id is a pure function of the annotation's identity — its kind, its whitespace-normalized text, and its enclosing site label (plus a source-order ordinal only when those three collide). The same annotation therefore mints the same id on every `aristo stamp`, which is what lets the index re-associate its prior status and proof instead of treating it as removed-then-new. Editing the covered CODE does not change the id — that is body-hash drift, tracked separately; only rewording the claim or renaming/moving the enclosing item does. The id stays inside the `aret_` namespace charset so it always parses.

<sub>Verify level: **test**</sub>

---
