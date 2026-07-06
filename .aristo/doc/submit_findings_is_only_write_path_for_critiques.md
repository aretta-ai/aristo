**Aristo verified intent — `submit_findings_is_only_write_path_for_critiques`**

`aristo critique --submit-findings` is the only path that creates a `.aristo/critiques/<id>.critique` file. On accept it prints `accepted: sha256:<hex>` to stdout for the orchestrator's integrity check. Every validation gate — the schema enums, the focal id existing in the current index, the text staleness anchor, and a non-empty rationale on each finding — runs first; if any fails, nothing is written.

<sub>Verify level: **neural**</sub>

---
