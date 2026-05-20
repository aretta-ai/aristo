**Aristo verified intent — `doc_include_graph_appends_idempotently`**

`aristo doc --include-graph` appends the rendered annotation graph (as a fenced ```mermaid block) to `_summary.md` AFTER the summary has been written. Order matters: appending after means a re-run with `--include-graph` produces the same file regardless of whether the prior run had the flag. A refactor that prepended or inserted-in-middle would make the output dependent on prior state, which makes `--check` brittle.

<sub>Verify level: **neural**</sub>

---
