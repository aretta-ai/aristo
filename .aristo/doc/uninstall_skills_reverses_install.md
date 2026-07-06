**Aristo verified intent — `uninstall_skills_reverses_install`**

Installing skills and then uninstalling them returns the project's on-disk state to exactly what it was before either ran, except for files the user hand-modified. For file-copy skills, the per-skill files that were written are removed; for AGENTS.md skills, the marker-delimited block is stripped and the surrounding content is preserved. Uninstalling when nothing is installed is a no-op.

<sub>Verify level: **test**</sub>

---
