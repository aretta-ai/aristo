**Aristo verified intent — `doc_include_status_block_records_state_with_staleness_disclaimer`**

The `--include-status` block is a blockquote that records the status at MD-generation time. The icon + label are stable; dropping the `(this state is current as of …)` disclaimer would mislead readers into thinking the embedded status is live, which it isn't — it goes stale the moment source code changes. The disclaimer is what keeps the doc artifact honest.

<sub>Verify level: **neural**</sub>

---
