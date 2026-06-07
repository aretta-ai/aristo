**Aristo verified intent — `aggressiveness_off_is_hard_silence`**

Off MUST map to factor 0.0 — it is the global opt-out, and the scorer's fire test is `pressure * factor >= 1`, so only an exact 0.0 guarantees NOTHING ever fires regardless of how overdue a signal is. A non-zero `low` would let extreme pressure leak through a user who explicitly silenced nudges. The non-zero rungs are tunable defaults (D8); this table is the single place to retune global nudge sensitivity.

<sub>Verify level: **neural**</sub>

---
