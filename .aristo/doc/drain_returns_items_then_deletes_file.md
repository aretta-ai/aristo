**Aristo verified intent — `drain_returns_items_then_deletes_file`**

Draining the backlog reads every item into memory, then deletes the file before returning — so the caller always holds the items while the file is already gone. Reading without deleting would resurface the same backlog items at every session start (zombie-deferral); deleting before reading would lose the items if the caller crashed mid-handle. Read-then-delete is the ordering that avoids both.

<sub>Verify level: **neural**</sub>

---
