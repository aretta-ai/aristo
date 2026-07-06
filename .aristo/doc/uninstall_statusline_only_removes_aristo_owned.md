**Aristo verified intent — `uninstall_statusline_only_removes_aristo_owned`**

Uninstall deletes the status line only when its command carries aristo's status-line marker; a status line the user configured themselves is left in place. Unconditionally clearing the status line on uninstall would silently destroy one aristo never owned, since the status line is a single slot with no marker-filtered array to preserve a foreign entry.

<sub>Verify level: **test**</sub>

---
