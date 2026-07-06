**Aristo verified intent — `aggressiveness_off_is_hard_silence`**

Off MUST map to factor zero — it is the global opt-out. The scorer fires only when a signal's pressure scaled by its factor reaches the firing threshold, so an exact zero is the only value that guarantees nothing ever fires no matter how overdue a signal is. Assigning Off any small but non-zero factor would let extreme pressure leak through to a user who deliberately silenced nudges. The non-zero levels are tunable defaults (D8); this table is the single place to retune global nudge sensitivity.

<sub>Verify level: **neural**</sub>

---
