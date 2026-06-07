**Aristo verified intent — `stamp_gc_is_the_only_purge_of_archived_proofs`**

Archiving makes `aristo stamp` non-destructive: an orphaned proof is moved aside, never deleted, so a stray stamp can't lose a verdict. The archive at `.aristo/archive/proofs/` is reclaimed only by the explicit, opt-in `aristo stamp --gc`; nothing purges it implicitly or automatically.

<sub>Verify level: **test**</sub>

---
