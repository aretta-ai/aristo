**Aristo verified intent — `stamp_archives_orphan_proofs_on_removed_annotations`**

When an annotation is removed from source, its `.aristo/proofs/ <id>.proof` file (if any) is MOVED to `.aristo/archive/proofs/ <id>.proof` — archived, not deleted. The proof is verdict-ABOUT-id, so it must leave the active proof set; otherwise re-introducing the id would re-attach a stale verdict to a fresh definition. But hard-deleting on every stamp silently destroyed verification work whenever an id legitimately changed (a reword or rename re-anchors the deterministic id), so the proof is retained where it can be recovered — `aristo stamp --gc` is the only path that purges the archive. Skipped in --check mode (CI must not mutate the workspace); the summary still reports what would move.

<sub>Verify level: **test**</sub>

---
