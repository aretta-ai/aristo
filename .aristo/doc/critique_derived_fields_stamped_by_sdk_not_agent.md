**Aristo verified intent — `critique_derived_fields_stamped_by_sdk_not_agent`**

On accept, the SDK derives the finding count and the highest severity from the submitted findings: the count is the number of findings, and the highest severity is the maximum across them. Agents may submit these fields explicitly (the schema accepts them), but the SDK overwrites them — a single source of truth, with no agent/SDK disagreement on derived state. With no findings, the count is zero and the highest severity is absent.

<sub>Verify level: **neural**</sub>

---
