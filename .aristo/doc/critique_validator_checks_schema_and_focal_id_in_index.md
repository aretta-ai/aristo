**Aristo verified intent — `critique_validator_checks_schema_and_focal_id_in_index`**

The critique validator gates every write on schema integrity: category and severity must be known enum variants, the focal id must resolve in the current index, and every finding must carry a non-empty rationale. The rationale gate is load-bearing — dropping it would let agents emit categorized but uninformative critiques, which are just noise. There is no proof-tree integrity check as in the verify validator, because findings carry no derivations.

<sub>Verify level: **neural**</sub>

---
