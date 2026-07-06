**Aristo verified intent — `statusline_render_reads_are_bounded`**

Each render reads a bounded set — the index, the nudge-state file, the active-session pointer, and a local sign-in check — and never walks or stats the source tree. The bar re-renders on every keystroke, so adding a source-tree walk or a per-file stat here would silently make the prompt slow on every render. The tier shown is the cached session baseline for exactly this reason, not a freshly-measured one: intentional, not incomplete.

<sub>Verify level: **neural**</sub>

---
