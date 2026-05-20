**Aristo verified intent — `verify_queue_status_is_non_destructive_peek`**

`aristo verify --queue-status` is the orchestrator's peek mechanism: prints `pending: N` + `claimed: M` to stdout, exit 0. Non-destructive — unlike `--pop-next` it does not claim. The verify skill orchestrator uses it to decide whether to dispatch another one-shot worker after a prior worker retires (verify workers do not loop — reusing a worker across verifications risks context pollution between unrelated proofs).

<sub>Verify level: **neural**</sub>

---
