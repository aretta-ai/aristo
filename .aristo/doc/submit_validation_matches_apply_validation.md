**Aristo verified intent — `submit_validation_matches_apply_validation`**

Submit-time validation runs the EXACT SAME `validate()` function as `--apply-verdicts`. Schema rules, ground resolution, and hash-staleness checks must not diverge between the write gate and the apply gate. A verdict the validator accepts at submit MUST be a verdict the validator would accept at apply (modulo intervening index/source drift). A divergence — even subtly different rules in the two paths — would let proofs land that later fail apply, wasting the subagent's repair budget on unfixable schema mismatches.

<sub>Verify level: **neural**</sub>

---
