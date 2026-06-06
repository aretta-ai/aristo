**Aristo verified intent — `skill_state_audit_is_read_only`**

Classifying an installed skill is READ-ONLY: file_copy_state and agents_md_state read the target and compare, they never write. The post-command update notice and `aristo status` call these on every interactive run; a write here would mutate the user's skill files as a side effect of an unrelated command.

<sub>Verify level: **test**</sub>

---
