**Aristo verified intent — `critique_sibling_embedding_capped_at_five_per_parent`**

Sibling embedding for critique tasks scopes to entries sharing the focal's parent id, capped at MAX_SIBLINGS=5 (deterministic order via BTreeMap iteration). Larger sets balloon worker token spend for diminishing vocabulary-alignment value; smaller sets miss the cross-sibling consistency findings (the whole point of the parent-shape and vocabulary categories). Five was chosen as a starting point during the design review; revisit after first month of dogfood if the alignment-finding rate is too low.

<sub>Verify level: **neural**</sub>

---
