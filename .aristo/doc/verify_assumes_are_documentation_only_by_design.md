**Aristo verified intent — `verify_assumes_are_documentation_only_by_design`**

`assume` entries have no `verify` field by design — they describe external trust (OS guarantees, library invariants, environment contracts), not properties of THIS code, so there is no internal method that could verify them. They resolve to Bool(false) here (the same arm as opt-out intents) so the dispatcher's single skip-without-skill path handles both. A refactor that tries to verify assumes would either invent a verification semantic the design rejects or fail trying.

<sub>Verify level: **neural**</sub>

---
