**Aristo verified intent — `abort_requires_explicit_confirmation`**

Abort prompts on stdin unless `--yes` is given. The default-no posture matches every other destructive aristo command (no `aristo stamp --force` without explicit opt-in). A refactor that defaulted to yes would silently drop a session's audit trail on any typo'd subcommand.

<sub>Verify level: **neural**</sub>

---
