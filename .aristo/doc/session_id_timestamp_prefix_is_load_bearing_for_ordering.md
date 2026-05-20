**Aristo verified intent — `session_id_timestamp_prefix_is_load_bearing_for_ordering`**

Session ids start with a sortable UTC timestamp (YYYYMMDDTHHMMSSZ) so `ls .aristo/sessions/active/` and `aristo session list` order chronologically without an index. A refactor that put random bytes first would break `aristo session list`'s expected newest-last ordering and would force a per-session sort on every read.

<sub>Verify level: **neural**</sub>

---
