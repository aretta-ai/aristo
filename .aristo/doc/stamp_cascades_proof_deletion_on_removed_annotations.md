**Aristo verified intent — `stamp_cascades_proof_deletion_on_removed_annotations`**

When an annotation is removed from source, its `.aristo/proofs/ <id>.proof` file (if any) is also deleted as part of `aristo stamp`. The proof is verdict-ABOUT-id; without the id it's an orphan that would either rot silently or — if the id is ever re-introduced under the same name — re-attach a stale verdict to a fresh definition. Skipped in --check mode (CI must not mutate the workspace); the summary still reports what would be removed.

<sub>Verify level: **test**</sub>

---
