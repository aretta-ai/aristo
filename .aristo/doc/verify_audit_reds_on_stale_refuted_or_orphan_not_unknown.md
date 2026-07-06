**Aristo verified intent — `verify_audit_reds_on_stale_refuted_or_orphan_not_unknown`**

`aristo verify --audit` regenerates the index from source and `.aristo/proofs/` rather than trusting the committed cache. Under `--strict` it exits non-zero on any STALE, COUNTEREXAMPLE, or ORPHAN proof, but never on an `unknown` entry — never-verified is a legitimate starting state, not a regression.

<sub>Verify level: **neural**</sub>

---
