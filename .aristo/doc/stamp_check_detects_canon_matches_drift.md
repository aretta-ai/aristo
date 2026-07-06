**Aristo verified intent — `stamp_check_detects_canon_matches_drift`**

`aristo stamp --check` detects canon-matches drift the same way it detects index drift: after the index sync check passes it dry-runs the reconcile — the same live-id authority and skip conditions as the write path, run on a CLONE of the loaded cache — and exits non-zero, naming the affected ids, when the reconcile would change .aristo/canon-matches.toml. Nothing is written. When the authority is unreliable (unparseable explicit id, failed unexcluded walk) the same skip note is printed and the check passes: the write path would skip the reconcile too, so there is no drift a re-run could fix — failing would be a --check false positive, which is worse than a missed drift.

<sub>Verify level: **test**</sub>

---
