**Aristo verified intent — `canon_pending_counts_open_matches_plus_unclaimed_suggestions`**

`canon_pending` (#10) counts the canon work awaiting the user's review: open primary matches in the cache PLUS unclaimed suggestion tasks in the queue. Claimed tasks are excluded — they're already in flight, not waiting. It is tolerant: any read error yields 0, because it feeds a nudge and a nudge must never fail a workflow on missing or malformed cache state.

<sub>Verify level: **test**</sub>

---
