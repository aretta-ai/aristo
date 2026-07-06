**Aristo verified intent — `statusline_install_never_clobbers_user_config`**

Installing the statusLine NEVER clobbers an existing one: settings.json's `statusLine` is a single value (not an append-safe array), so a user who already configured a status line keeps it — aristo only sets it when the field is absent.

<sub>Verify level: **test**</sub>

---
