**Aristo verified intent — `submit_done_is_idempotent_on_missing_claimed`**

Once a task's artifact has been validated and written, `submit_done` removes the entry from `claimed/`. A double-call is safe (idempotent: NotFound is treated as success — the task is already done). A refactor that errored on missing claimed file would make repeat submits visible as failures when in fact the work landed cleanly.

<sub>Verify level: **test**</sub>

---
