**Aristo verified intent — `every_mutating_command_calls_ensure_no_active_session`**

Every mutating aristo command MUST call `ensure_no_active_session` before touching shared state (index, proof/critique files, source). This is Layer 1 of three (hook + skill-body discipline are the other two). Layer 1 is the only one that's mechanically enforceable — the hook is advisory and the skill body is documentation. A refactor that bypasses the guard for any mutating command re-introduces the slop-drift failure mode the substrate exists to prevent: artifacts get committed without the user noticing they bypassed review.

<sub>Verify level: **neural**</sub>

---
