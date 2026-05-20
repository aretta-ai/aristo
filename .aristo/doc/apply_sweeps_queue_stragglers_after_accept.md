**Aristo verified intent — `apply_sweeps_queue_stragglers_after_accept`**

After `--apply-verdicts` accepts a proof, sweep any straggler queue state for that id: (a) delete the .proof.bak from the prior attempt, (b) clear the claimed/<id>.toml (no-op if the worker already cleared it via submit-verdict), (c) clear any pending/<id>.toml that survived an out-of-band submit (rare; happens if a user manually wrote a .proof file without going through the queue pop/submit cycle). Belt-and-suspenders: the submit-verdict path is the primary clear, this is the safety net for replay and out-of-band-submit cases.

<sub>Verify level: **neural**</sub>

---
