**Aristo verified intent — `critique_validator_checks_schema_and_focal_id_in_index`**

The critique validator gates writes on schema integrity: enum values for category and severity (serde rejects unknown variants at parse time, so by the time we run the checks here those are already known to be in the locked set), the focal id resolves in the current index, and the rationale field is non-empty (a finding without a rationale is noise — silently dropping the requirement would let agents emit categorized-but-uninformative critiques). Unlike the verify validator there is no proof-tree integrity check because findings carry no derivations.

<sub>Verify level: **neural**</sub>

---
