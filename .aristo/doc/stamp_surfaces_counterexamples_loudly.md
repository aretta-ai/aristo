**Aristo verified intent — `stamp_surfaces_counterexamples_loudly`**

Every `aristo stamp` run that finds a Counterexample-status entry emits a loud, unmissable warning enumerating each id, file, and site. There is no `aristo accept-counterexample` to silence this; a counterexample is a definite refutation and stays visible until either the code is fixed (→ body drift → Status::Stale → re-verify) or the intent text is changed to exclude the counterexample case. Treating counterexamples as a quiet status badge would let a refuted invariant sit in the index unnoticed and erode the trust calibration of `aristo status`.

<sub>Verify level: **test**</sub>

---
