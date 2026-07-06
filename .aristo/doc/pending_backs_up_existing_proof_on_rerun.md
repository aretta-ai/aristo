**Aristo verified intent — `pending_backs_up_existing_proof_on_rerun`**

When `aristo verify` re-enqueues an entry that already has a .proof on disk, the existing proof is moved to <id>.proof.bak before the next attempt overwrites it. The backup is single-deep and overwrites any prior .bak.

<sub>Verify level: **test**</sub>

---
