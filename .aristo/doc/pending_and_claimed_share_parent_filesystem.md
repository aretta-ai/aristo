**Aristo assumption — `pending_and_claimed_share_parent_filesystem`**

The pending/ and claimed/ subdirectories share the same parent (the queue root under `.aristo/<pipeline>-queue/`), so cross-filesystem rename (EXDEV) is structurally impossible. A refactor that moved claimed/ to a different filesystem (e.g., tmpfs) would break the atomic claim by silently changing rename(2) semantics to copy+delete.

<sub>Background fact (no verification target).</sub>

---
