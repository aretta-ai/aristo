**Aristo verified intent — `active_pointer_read_trims_whitespace`**

`.active` is the single source of truth for 'is there an active session?'. Reading it is the first step of every SDK pre-check. Whitespace-trim the contents so a stray trailing newline (from an editor or a shell `echo`) doesn't break id lookup. A refactor that used the contents verbatim would silently treat an edited pointer as no-such-session.

<sub>Verify level: **neural**</sub>

---
