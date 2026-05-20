**Aristo verified intent — `pop_next_uses_atomic_rename_for_race_safety`**

`pop_next` atomically claims one pending entry by renaming `pending/<id>.toml` → `claimed/<id>.toml`. Two workers racing on the same entry cannot both succeed: POSIX rename guarantees the source path disappears after the first call returns, so the loser sees ENOENT and tries the next entry from a freshly-listed pending/. The function returns Ok(None) ONLY when a fresh listing of pending/ turns up empty (queue genuinely drained); a non-empty listing where every entry was claimed by others triggers a re-list, not a None return. A refactor that short-circuits to None on first ENOENT would falsely report 'queue drained' under concurrent load.

<sub>Verify level: **neural**</sub>

---
