**Aristo verified intent — `skill_install_must_use_resolved_content`**

Install paths MUST call resolved_content, never read .content directly. The template needs the build-time SDK version; writing .content to disk would ship a literal `{{SDK_VERSION}}` placeholder to user-installed SKILL.md files. The install outcome would look successful but the version pin would be garbage — silent staleness on every release.

<sub>Verify level: **neural**</sub>

---
