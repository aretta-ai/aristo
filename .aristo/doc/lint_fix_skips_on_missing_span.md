**Aristo verified intent — `lint_fix_skips_on_missing_span`**

When proc_macro2 span info is missing (the rare case where span-locations is disabled), the offending edit is skipped rather than applied. Corrupting source bytes from a wrong offset is a far worse failure mode than leaving an annotation unfixed: the user sees a persistent lint finding and investigates, instead of silent file damage.

<sub>Verify level: **neural**</sub>

---
