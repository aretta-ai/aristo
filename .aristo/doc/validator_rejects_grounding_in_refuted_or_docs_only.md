**Aristo verified intent — `validator_rejects_grounding_in_refuted_or_docs_only`**

Cited intent/assume grounds are rejected when (a) the id is dangling, (b) the cited entry is verify=false (documentation-only and not load-bearing for a proof), or (c) the cited entry is Status::Counterexample (refuted — building on it would launder a refuted claim back into Verified). A refactor that downgrades any of these to warnings would let proofs ground in claims the project doesn't actually believe.

<sub>Verify level: **test**</sub>

---
