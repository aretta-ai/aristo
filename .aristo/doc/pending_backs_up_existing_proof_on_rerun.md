**Aristo verified intent — `pending_backs_up_existing_proof_on_rerun`**

When `aristo verify` re-enqueues an entry that already has a .proof on disk, move the existing proof to <id>.proof.bak before the next attempt overwrites it. Single-deep backup — overwrites any prior .bak. Lets the user diff a rejected re-attempt against the prior verdict. The .bak is auto-deleted on successful --apply-verdicts.

<sub>Verify level: **test**</sub>

---
