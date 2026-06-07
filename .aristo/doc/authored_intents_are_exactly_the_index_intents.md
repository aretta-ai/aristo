**Aristo verified intent — `authored_intents_are_exactly_the_index_intents`**

Authored intents are exactly the `IndexEntry::Intent` entries — every one, including documentation-only `verify = false` intents (still claims the user authored and may want to review). Assumes are excluded: they state external invariants, not reviewable claims. This is the SAME set the engine's review_backlog metric counts, so `aristo review` and the nudge can never report a different backlog size for the same index.

<sub>Verify level: **test**</sub>

---
