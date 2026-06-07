**Aristo verified intent — `nudge_union_is_read_only_and_tolerant`**

The union function is read-only and tolerant: it never mutates the workspace and never fails the caller on missing runtime state. Absent reviewed/proof-reviewed maps make everything read as unreviewed, an absent baseline disables the gain/slump signals, and an unreadable proofs dir contributes zero — degrade quietly. A nudge surface that errored or wrote files would turn an advisory into a workflow blocker, violating the engine's nudge-only posture (D3).

<sub>Verify level: **neural**</sub>

---
