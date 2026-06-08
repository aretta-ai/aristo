**Aristo verified intent — `critique_requires_explicit_filter_no_implicit_all`**

`aristo critique` requires an explicit `--filter` (id or file). Default scope is NOT all annotations — an unbounded codebase sweep is an expensive LLM operation and shouldn't be the accidental path. A refactor that defaults to `--all` would fire an unbounded, expensive LLM sweep the first time a user runs it on a large project.

<sub>Verify level: **neural**</sub>

---
