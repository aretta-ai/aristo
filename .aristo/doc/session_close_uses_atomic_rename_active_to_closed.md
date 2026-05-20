**Aristo verified intent — `session_close_uses_atomic_rename_active_to_closed`**

Closing a session is an atomic file move from active/ to closed/. A reader that sees the file is gone from active/ MUST find it in closed/ (or .active was cleared first — see `[[clear_active_pointer]]`). A refactor that copied + deleted instead of renaming would introduce a window where the session exists in both directories (stale-read risk) or in neither (lost audit trail).

<sub>Verify level: **neural**</sub>

---
