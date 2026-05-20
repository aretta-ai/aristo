**Aristo verified intent — `critique_derived_fields_stamped_by_sdk_not_agent`**

On accept, the SDK derives `finding_count` from `findings.len()` and `highest_severity` from `findings.iter().map(|f| f.severity).max()`. Agents may submit these fields explicitly (the schema accepts them) but the SDK overwrites — single source of truth, no agent/SDK disagreement on derived state. None when findings is empty.

<sub>Verify level: **neural**</sub>

---
