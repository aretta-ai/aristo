**Aristo verified intent — `statusline_staleness_is_cheap_and_conservative`**

The bar's staleness count is cheap and conservative: an intent is stale if it is recorded broken (Status::Stale or Counterexample) OR it is currently terminal-clean but its source file's mtime is newer than the index's (edited since the last stamp, so its proof may be clobbered). The mtime test is a FILE-level heuristic — it over-counts versus a per-function body-hash recompute, which is the right bias for a 're-verify' warning and avoids the per-render source parse the bar forbids. When either mtime is unreadable it does NOT warn (tolerant: omission over a false alarm). Unknown/Inconclusive are not stale, only unverified.

<sub>Verify level: **test**</sub>

---
