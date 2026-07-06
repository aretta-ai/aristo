**Aristo verified intent — `c_effective_top_level_flattens_preproc_guards`**

The effective top-level items of a C file are flattened THROUGH conditional-compilation wrappers, because tree-sitter nests everything inside an `#ifndef`/`#define` header guard (or any `#if`/`#ifdef`) in a `preproc_ifdef`/`preproc_if` node — so a plain walk of the root's direct children misses every declaration in a guarded header, which is essentially every real C header. We cannot evaluate the conditions, so we flatten EVERY branch (matching how the extractor already reads raw source without preprocessing), and we drop the preprocessor's own infrastructure nodes (the guard name, `#define`, `#include`) so they never split a directive run. Reverting to a direct-children walk silently re-hides all header annotations.

<sub>Verify level: **test**</sub>

---
