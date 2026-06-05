**Aristo verified intent — `uninstall_claude_hook_preserves_other_hooks`**

Uninstall removes ONLY the aristo entry matching the given marker from its hook-event array; any other hooks the user configured are preserved. After removal an empty array is left in place rather than removed — the user may have intentional structure around the key. Idempotent: uninstalling-when-not-installed is a no-op return.

<sub>Verify level: **neural**</sub>

---
