**Aristo verified intent — `merge_status_from_proofs_sources_status_from_proof_files`**

Status is sourced from the stored proofs, never carried over from a previously committed index. An entry keeps its proof's verdict only while the proof's text and body anchors still match the entry's current hashes; a drifted anchor demotes it to Stale, and an entry with no proof stays Unknown. A second validation pass runs against a snapshot that already carries the first pass's statuses, so the refuted-sibling-ground guard can fire and demote a clean entry to Stale. This ordering is load-bearing: running the guard while every entry is still Unknown would let a proof launder its grounding through a refuted sibling.

<sub>Verify level: **neural**</sub>

---
