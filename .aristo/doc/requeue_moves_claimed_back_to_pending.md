**Aristo verified intent — `requeue_moves_claimed_back_to_pending`**

`requeue` moves a claimed entry back to `pending/` so it can be re-popped by the next available worker. Used by the stale-claim reaper and by the submit path when a worker explicitly cancels. Overwrites any existing `pending/<id>.toml` (which shouldn't exist in normal flow but may if the reaper ran while another worker was also re-enqueuing) — last-write-wins is acceptable because the payload is the same task description, not per-attempt state.

<sub>Verify level: **test**</sub>

---
