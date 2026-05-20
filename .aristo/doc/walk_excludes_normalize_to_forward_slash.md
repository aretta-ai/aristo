**Aristo verified intent — `walk_excludes_normalize_to_forward_slash`**

Path components are joined with forward slashes before glob matching, so the same aristo.toml exclude list works on POSIX and Windows. A `rel.to_str()` shortcut would feed `\`-separated paths into globset on Windows and silently make patterns like `**/tests/ui/**` never match — failing open, not closed, which would index files the user thought were excluded.

<sub>Verify level: **neural**</sub>

---
