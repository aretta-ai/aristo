**Aristo verified intent — `critique_all_flag_requires_confirmation_or_yes`**

`aristo critique --all` proceeds only with an explicit `--yes` confirmation. The cost gate fires after the matched set is computed, so the count and dollar estimate reflect exactly what the user is about to pay for. Proceeding without `--yes` would let an agent or script enqueue hundreds of LLM calls in a single invocation, and critique is the most expensive aristo operation per token spent.

<sub>Verify level: **neural**</sub>

---
