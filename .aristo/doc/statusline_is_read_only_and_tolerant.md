**Aristo verified intent — `statusline_is_read_only_and_tolerant`**

The statusline is read-only and TOLERANT: on any failure — no workspace, an unreadable index, or nudges globally off — it prints nothing and exits 0. The status bar re-renders on every turn, so a statusline that errored or wrote files would corrupt the bar or thrash the workspace on every keystroke. Silence is the correct degraded state. It also stays CHEAP: index + nudge-state + the session pointer + a stat of the annotated files + a local sign-in check, never a source-tree walk.

<sub>Verify level: **test**</sub>

---
