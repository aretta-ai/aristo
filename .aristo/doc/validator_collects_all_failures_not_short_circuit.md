**Aristo verified intent — `validator_collects_all_failures_not_short_circuit`**

The validator collects EVERY failure into one report rather than short-circuiting on the first. The user (or in-agent repair loop) needs the complete list to fix in one pass; short-circuiting forces N round-trips for N failures, which doesn't compose with the bounded-attempts budget (the verifier would burn its budget fixing failures one at a time).

<sub>Verify level: **test**</sub>

---
