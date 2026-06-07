**Aristo verified intent — `metrics_verifiable_excludes_assumes_and_doc_only_intents`**

The verifiable surface is intents with `verify != false` only — assumes are external invariants and never verified, and `verify = false` intents are documentation-only. `verified_clean` counts the verifiable intents in a terminal-clean status (the shared `Status::is_terminal_clean`), and `unverified` is exactly `verifiable - verified_clean`, so the three never disagree. The rate divides by `verifiable` (not by all intents), and is 0 when nothing is verifiable rather than a divide-by-zero.

<sub>Verify level: **test**</sub>

---
