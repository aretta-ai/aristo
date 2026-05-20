**Aristo verified intent — `uninstall_skills_reverses_install`**

`install_skills` followed by `uninstall_skills` leaves the project's relevant on-disk state identical to before either ran (modulo files the user hand-modified). File-copy agents: the per-skill files we wrote are removed. AGENTS.md agents: the marker-delimited block is stripped; surrounding content preserved. Idempotent on uninstall-while-uninstalled.

<sub>Verify level: **test**</sub>

---
