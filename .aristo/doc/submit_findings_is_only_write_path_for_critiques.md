**Aristo verified intent — `submit_findings_is_only_write_path_for_critiques`**

`aristo critique --submit-findings` is the SINGLE creation path for `.aristo/critiques/<id>.critique` files (subagents have no Write-tool access — critique workers have Bash only). On accept, prints `accepted: sha256:<hex>` to stdout for the orchestrator's integrity check. Validation gates schema enums + focal-id-in-index + text staleness anchor + per-finding rationale presence; any failure short-circuits before write_proof_atomic.

<sub>Verify level: **neural**</sub>

---
