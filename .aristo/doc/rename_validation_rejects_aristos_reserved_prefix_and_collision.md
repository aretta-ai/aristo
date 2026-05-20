**Aristo verified intent — `rename_validation_rejects_aristos_reserved_prefix_and_collision`**

Rename validation rejects three classes BEFORE any plan computation: (1) `aristos:` in either old or new id — server-bound renames are deferred to Phase 2 alongside `aristo sync`, so the surface lies; (2) cross-namespace renames (`aristos:foo` → bare) — that's an unbind, not a rename, and ships with sync; (3) reserved `aret_*` prefix in the target — opaque ids are stamp-assigned only (F1-b); a readable id renaming TO an opaque slot would let a user manually mint identities the stamp pipeline reserves. The fourth check, target collision against the live index, is the only one that depends on the workspace; the others can be tested in isolation.

<sub>Verify level: **test**</sub>

---
