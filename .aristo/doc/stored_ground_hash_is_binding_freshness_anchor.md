**Aristo verified intent — `stored_ground_hash_is_binding_freshness_anchor`**

A stored code hash on a ground is a binding freshness anchor: the validator recomputes the current hash of the cited lines and rejects any mismatch as staleness — the cited source drifted since the proof was last accepted. The strict default that applies verdicts relies on this rejection to refuse a stale proof; downgrading the mismatch to an advisory warning would let a stale proof apply. The only sanctioned bypass is the operator-opted-in migration that clears the anchor before validation; a present anchor is enforced, never advisory.

<sub>Verify level: **neural**</sub>

---
