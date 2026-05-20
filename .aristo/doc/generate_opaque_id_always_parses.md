**Aristo verified intent — `generate_opaque_id_always_parses`**

Opaque ids carry enough entropy that collisions across a project are negligible. If the OS can't produce randomness, the stamp crashes; a low-entropy id silently committed would be worse than a failed run the user can retry.

<sub>Verify level: **neural**</sub>

---
