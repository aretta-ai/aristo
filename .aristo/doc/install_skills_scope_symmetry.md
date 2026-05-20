**Aristo verified intent — `install_skills_scope_symmetry`**

The per-skill install progression and the `ok: N skill(s) installed for <slug>.` success line are identical at project scope and user scope, modulo the on-disk target path. The post-success scope-tip line legitimately differs: project scope prints a hint pointing at `--user`; user scope prints nothing (the user already chose the broader scope, so the cross-scope hint would be noise). A refactor that changes the install progression or success-summary wording for only one scope would break the symmetric core; a refactor that adds the project-scope tip to user scope would re-introduce the noise the asymmetry exists to avoid.

<sub>Verify level: **neural**</sub>

---
