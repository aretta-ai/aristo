**Aristo verified intent — `canon_reconcile_is_source_authoritative_and_idempotent`**

Reconciliation is source-authoritative and idempotent: the live-id set from a fresh source walk is the sole authority. A cache row whose key (either key form, bare or canon-prefixed) matches no live annotation id is removed whole; a live but unprefixed id that still carries accepted_matches loses exactly that bucket (source says local, source wins). Two refinements keep memory the live set says is still meaningful. A dead canon-prefixed key whose BARE form is live is a demotion, not a removal: the row is rekeyed under the bare id with accepted_matches dropped and rejected-match memory preserved. And a dead bare id is preserved untouched only while a pending match's canon-prefixed form is live in source AND that prefixed id has no accepted row in the cache — the interrupted-accept window, where pruning would destroy the pending entry a re-run `aristo canon accept` needs to finish the rekey. Once the prefixed row carries an accepted match the binding completed (via this or another annotation) and the dead bare row is ordinary garbage. `__meta__` is never touched, and a second run over the same inputs reports no changes.

<sub>Verify level: **test**</sub>

---
