**Aristo verified intent — `canon_merge_response_preserves_user_decisions`**

Merging match response into cache is per-annotation idempotent: each batched annotation's candidate list replaces ONLY that annotation's `pending_matches`; `accepted_matches` and `rejected_matches` for the same annotation are untouched (user decisions survive). A regression that overwrote accepted/rejected here would silently undo the user's review work on every stamp.

<sub>Verify level: **test**</sub>

---
