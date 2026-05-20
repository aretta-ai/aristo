**Aristo verified intent — `drain_returns_items_then_deletes_file`**

Draining the backlog atomically removes the file after returning its contents — once the caller has the items, the file is gone. The pattern matches `aristo verify --apply-verdicts`: read all, mutate, the artifact is consumed. A refactor that read without deleting would surface the same backlog items every session start, creating zombie-deferral. A refactor that deleted before returning would lose data if the caller crashed mid-handle — read-then-delete keeps the items in memory while the file is gone.

<sub>Verify level: **neural**</sub>

---
