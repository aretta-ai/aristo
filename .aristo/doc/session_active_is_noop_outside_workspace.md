**Aristo verified intent — `session_active_is_noop_outside_workspace`**

`aristo session active` is wired into Claude Code's UserPromptSubmit hook (Layer 2). The hook fires on EVERY prompt across EVERY project, not only aristo workspaces. So `active` must exit 0 with empty stdout when run outside an aristo workspace — a hard error would block every prompt in any non-aristo project the user opens. The interactive form (no `--hook-format`) follows the same rule for symmetry: an active session can only exist within a workspace anyway, so no-workspace and no-session are observationally identical at the CLI surface.

<sub>Verify level: **neural**</sub>

---
