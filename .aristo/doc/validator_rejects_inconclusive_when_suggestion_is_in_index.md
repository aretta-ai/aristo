**Aristo verified intent — `validator_rejects_inconclusive_when_suggestion_is_in_index`**

An inconclusive verdict is stale once any of its suggested annotations is present in the current index (text-hash match). Either the user adopted the suggestion (good — re-run to see if the gap closes) or the agent missed an existing entry that would have closed the gap (good — re-run with the entry available as a ground). Both paths converge on 'this verdict is no longer the best answer; re-verify'. Without this check, adopting a suggestion never moves the entry back to pending — the user adds the assume the agent asked for, and aristo verify silently skips it forever.

<sub>Verify level: **test**</sub>

---
