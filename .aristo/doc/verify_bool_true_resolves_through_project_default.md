**Aristo verified intent — `verify_bool_true_resolves_through_project_default`**

Bool(true) resolves through the project's [verify].default_method and falls back to the free-tier default ("test") when absent. A refactor that hard-codes either side would silently change verification depth for every annotation that opted into the project default — those are precisely the entries where the author deferred to project policy, so a silent override defeats the deferral.

<sub>Verify level: **neural**</sub>

---
