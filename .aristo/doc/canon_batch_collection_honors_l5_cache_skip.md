**Aristo verified intent — `canon_batch_collection_honors_l5_cache_skip`**

An annotation is added to the canon-match batch when (a) the user passed --refresh-canon, OR (b) no cached entry exists yet, OR (c) the cached entry's last_match_text_hash differs from the current annotation text_hash. A fresh cache hit produces no API traffic — load-bearing for the daily-loop UX where most stamps touch nothing canon-relevant.

<sub>Verify level: **test**</sub>

---
