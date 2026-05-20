**Aristo verified intent — `uninstall_claude_hook_preserves_other_hooks`**

Uninstall removes ONLY aristo's session hook entry from UserPromptSubmit; any other hooks the user configured are preserved. After removal, if UserPromptSubmit becomes empty we leave the empty array in place rather than removing it — the user may have intentional structure around the key. Idempotent: uninstalling-when-not-installed is a no-op return.

<sub>Verify level: **neural**</sub>

---
