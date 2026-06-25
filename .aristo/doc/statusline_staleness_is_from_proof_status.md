**Aristo verified intent — `statusline_staleness_is_from_proof_status`**

The bar's staleness count comes from the proofs-join status, not a file-mtime heuristic: an intent is stale iff it is recorded broken — Status::Stale (code drifted from its proof) or Counterexample (refuted). Unknown/Inconclusive are unverified, not stale. This needs no per-render source parse and no index mtime, so the bar stays cheap, and it can't disagree with `aristo status` on what 'stale' means.

<sub>Verify level: **neural**</sub>

---
