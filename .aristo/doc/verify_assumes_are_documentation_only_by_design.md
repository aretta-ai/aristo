**Aristo verified intent — `verify_assumes_are_documentation_only_by_design`**

`assume` entries have no `verify` field by design — they describe external trust (OS guarantees, library invariants, environment contracts), not properties of THIS code, so there is no internal method that could verify them. They take the same skip-without-verification path the dispatcher uses for opt-out intents, so neither needs a verification skill. A refactor that tries to verify assumes would either invent a verification semantic the design rejects or fail trying.

<sub>Verify level: **neural**</sub>

---
