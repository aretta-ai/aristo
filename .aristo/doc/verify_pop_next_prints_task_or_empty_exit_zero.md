**Aristo verified intent — `verify_pop_next_prints_task_or_empty_exit_zero`**

`aristo verify --pop-next` is the worker-facing API: it atomically claims one pending task from `.aristo/verify-queue/pending/`, prints the task body (TOML) to stdout, and exits 0. When the queue is genuinely drained, it prints nothing and still exits 0 — the caller distinguishes 'drained' from 'task body' by checking whether stdout is empty. A refactor that printed a sentinel string (e.g., 'empty') on a drained queue would collide with any task content; a refactor that exited non-zero would force every worker to special-case the happy path. Print-or-empty is the contract.

<sub>Verify level: **neural**</sub>

---
