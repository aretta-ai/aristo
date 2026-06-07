**Aristo verified intent — `nudge_review_is_current_only_until_hash_drift`**

A review is current only while the annotation's text AND body hashes still match the snapshot taken when it was reviewed. Editing the claim or the covered code after review re-opens it (reads as unreviewed) — a reviewer approved a specific version, not the id forever. Without the hash check, a post-review edit would keep a stale 'reviewed' badge and the review backlog would under-count work that genuinely needs another look.

<sub>Verify level: **test**</sub>

---
