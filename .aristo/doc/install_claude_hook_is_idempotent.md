**Aristo verified intent — `install_claude_hook_is_idempotent`**

Installing the session hook is idempotent — running install twice leaves the settings.json with exactly one `aristo session active --hook-format` entry, not two. We find existing entries by command substring and replace in place. A refactor that appends unconditionally would compound on every reinstall.

<sub>Verify level: **neural**</sub>

---
