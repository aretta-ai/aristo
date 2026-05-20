**Aristo verified intent — `verify_skip_consults_validator_not_just_status`**

An entry is skipped iff (1) its status is in the terminal set {Verified, Tested, Neural, Counterexample, Inconclusive} AND (2) its on-disk .proof file still passes the mechanical validator against the current index + source. The validator is the source of truth for 'still applicable'; the status flag is a cache. Reading only the flag would miss ground drift in existing proofs (cited code rewritten, cited intent's text changed) — invisible until the next --apply-verdicts cycle. Re-running the validator at list time is bounded by the count of terminal entries, which is the workload we are trying to skip.

<sub>Verify level: **test**</sub>

---
