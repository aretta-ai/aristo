**Aristo verified intent — `verify_queue_status_is_non_destructive_peek`**

`aristo verify --queue-status` is a non-destructive peek: it prints `pending: N` and `claimed: M` to stdout and exits 0. Unlike `--pop-next`, it does not claim any entry.

<sub>Verify level: **neural**</sub>

---
