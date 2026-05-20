**Aristo verified intent — `submit_verdict_is_only_write_path_for_proofs`**

`aristo verify --submit-verdict` is the SINGLE *creation* path for `.aristo/proofs/<id>.proof` files (subagents have no Write-tool access). `--apply-verdicts` may re-write an existing proof in-place to stamp computed ground hashes, but only after running the same `validate()` schema gate. A refactor that added a third writer bypassing `validate()` would let unvalidated proofs land — defeating the schema gate that catches invalid enum variants, child-as-prior-step, and out-of-range line citations.

<sub>Verify level: **neural**</sub>

---
