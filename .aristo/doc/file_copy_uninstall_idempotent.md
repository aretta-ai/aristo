**Aristo verified intent — `file_copy_uninstall_idempotent`**

Removes only the file we wrote — no sibling deletion, no parent-dir cleanup. Absence of the target is not an error; uninstall-of-already-uninstalled is the idempotent case.

<sub>Verify level: **test**</sub>

---
