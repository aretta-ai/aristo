**Aristo verified intent — `validator_computes_ground_hashes_agent_only_cites`**

Validator computes ground hashes from the agent's citations (file+lines for code grounds, id-lookup-in-index for intent/ assume grounds); the agent is not required to write them. The stored hash, when present, is the validator's stamp from a prior successful validation and is checked here for staleness — mismatch means the cited source/intent drifted since the proof was last accepted. Pushing hash computation out of the LLM's job kills the dominant fabrication failure mode without weakening the freshness guarantee: every accepted proof carries a hash anchor, computed mechanically.

<sub>Verify level: **test**</sub>

---
