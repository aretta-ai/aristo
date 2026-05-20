**Aristo verified intent — `workspace_load_config_degrades_to_default`**

Malformed or missing aristo.toml degrades to ConfigFile::default() rather than erroring. Reader commands stay functional with project defaults when the user's config has a typo. A refactor that propagates errors here would break every reader (show / list / status / lint) at first typo. Commands that need parse errors surfaced must read and parse directly.

<sub>Verify level: **neural**</sub>

---
