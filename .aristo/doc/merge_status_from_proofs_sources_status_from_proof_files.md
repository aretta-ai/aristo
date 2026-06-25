**Aristo verified intent — `merge_status_from_proofs_sources_status_from_proof_files`**

Status is sourced from `.aristo/proofs/`, not carried from a prior committed index. For each entry the matching `.proof` is loaded and its `produced_at_text_hash`/`produced_at_body_hash` anchors checked against the entry's current hashes: anchors valid -> the proof's verdict; drifted -> Stale; no proof -> Unknown (left as built). A second pass re-runs the full validator against a snapshot carrying the first pass's statuses, so the refuted-sibling-ground guard fires and a clean status is demoted to Stale when the proof no longer validates. This is the source of truth once the index becomes a gitignored cache; running it while every entry is still Unknown would let a proof launder grounding in a refuted sibling, so the two-phase ordering is load-bearing.

<sub>Verify level: **neural**</sub>

---
