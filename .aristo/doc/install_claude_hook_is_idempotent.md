**Aristo verified intent — `install_claude_hook_is_idempotent`**

Installing a hook is idempotent — running install twice leaves settings.json with exactly one entry for that hook's marker, not two. Existing entries are found by command substring and left in place. A refactor that appends unconditionally would compound on every reinstall. Applies uniformly to every hook event (UserPromptSubmit, SessionStart).

<sub>Verify level: **neural**</sub>

---
