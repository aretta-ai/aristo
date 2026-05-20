**Aristo verified intent — `pending_carries_prior_attempts_from_existing_proof`**

Prior attempts for an id come from the existing `.aristo/proofs/<id>.proof` file (if any), parsed once to extract verdict.attempts. Carrying this across re-spawns activates the K-bounded repair budget that would otherwise be dead code: each fresh subagent invocation writing attempts=1 means a hard-to-verify intent can re-spawn indefinitely without ever hitting the cap. Reading from the rejected proof on disk is the only persistence channel available — the SDK doesn't track per-entry attempt history elsewhere.

<sub>Verify level: **test**</sub>

---
