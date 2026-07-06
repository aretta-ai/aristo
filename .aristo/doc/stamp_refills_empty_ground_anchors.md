**Aristo verified intent — `stamp_refills_empty_ground_anchors`**

After a verdict passes validation, every ground whose freshness anchor is empty is refilled with a hash recomputed from the current state it references — the annotation's text or the cited source lines — so the persisted proof always carries an anchor for later staleness checks. An empty anchor is never left in place, whether the proof was authored without one or the migration path cleared it beforehand. Narrowing this refill to a subset of ground kinds, or running it only when something was explicitly cleared, would silently leave proofs unanchored and disable their staleness detection.

<sub>Verify level: **test**</sub>

---
