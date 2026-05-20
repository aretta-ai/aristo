**Aristo verified intent — `clear_active_pointer_is_idempotent_and_load_bearing_for_exit`**

Clearing `.active` is the last step of every session-exit flow (strict exit, defer-undecided exit, and abort all clear it). A refactor that left `.active` pointing at a closed session would break every subsequent pre-check — the SDK would think a session is in flight that the user already exited. Idempotent (missing file is fine) so re-running an exit handler doesn't error.

<sub>Verify level: **neural**</sub>

---
