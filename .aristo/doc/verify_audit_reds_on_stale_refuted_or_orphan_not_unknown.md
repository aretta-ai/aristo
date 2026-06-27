**Aristo verified intent — `verify_audit_reds_on_stale_refuted_or_orphan_not_unknown`**

`verify --audit` is the freshness gate that replaces `aristo stamp --check` once the index is a gitignored cache. It regenerates the index from source + `.aristo/proofs/` (never trusting a possibly-stale committed cache) and, under `--strict`, exits non-zero on any STALE (code drifted since verification), COUNTEREXAMPLE (refuted), or ORPHAN proof (a `.proof` whose annotation no longer exists). It deliberately does NOT fail on `unknown` (never-verified is a legitimate starting state, not a regression). A deleted `.proof` surfaces as the now-`unknown` entry plus a tracked-file deletion in the diff, not as an audit failure.

<sub>Verify level: **neural**</sub>

---
