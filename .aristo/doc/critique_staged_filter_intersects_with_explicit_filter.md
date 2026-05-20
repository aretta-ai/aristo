**Aristo verified intent — `critique_staged_filter_intersects_with_explicit_filter`**

`aristo critique --staged` intersects with any `--filter file=` clauses rather than replacing them. Composition semantic, not replacement: `--staged --filter file=src/x.rs` enqueues the intersection (annotations in src/x.rs that ALSO appear in the git-staged set), not the union. A refactor that unions them would turn `--staged` into a quiet expansion that ignores explicit scoping the user added, contradicting its scoping-tighter purpose. Empty intersection (no staged files match the filter) yields the usual `0 annotations matched` exit, not an error.

<sub>Verify level: **neural**</sub>

---
