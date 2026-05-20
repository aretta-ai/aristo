**Aristo verified intent — `apply_findings_filters_open_by_default`**

`aristo critique --apply-findings` defaults to listing only findings whose `disposition` is `None` (open / not yet reviewed). Findings the user has already accepted, rejected, or deferred via `aristo session decide` stop re-surfacing on every apply — that's how the review substrate closes the loop. A refactor that re-surfaces every finding by default breaks the user's "I already triaged this" assumption and re-introduces the noise the substrate exists to filter. `--include-closed` is the explicit opt-back-in.

<sub>Verify level: **neural**</sub>

---
