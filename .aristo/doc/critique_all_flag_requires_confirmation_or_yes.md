**Aristo verified intent — `critique_all_flag_requires_confirmation_or_yes`**

`aristo critique --all` requires explicit confirmation via `--yes` (or interactive Y/N — interactive is parked for v2; v1 requires `--yes` on the command line). The cost-gate fires AFTER the matched set is computed so the count and dollar estimate match what the user is actually about to pay for. A refactor that proceeds without `--yes` would let an agent (or a script) accidentally enqueue hundreds of LLM calls in one bash invocation — the gate exists because critique is the most expensive aristo operation per token spent.

<sub>Verify level: **neural**</sub>

---
