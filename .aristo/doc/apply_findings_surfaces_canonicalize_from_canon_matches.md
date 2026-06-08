**Aristo verified intent — `apply_findings_surfaces_canonicalize_from_canon_matches`**

Canonicalize findings are surfaced alongside agentic critique findings in `aristo critique --apply-findings` — they originate from `.aristo/canon-matches.toml::pending_matches`, not from the `.critique` files the other five categories live in. A regression that read only the .critique files would silently hide every canon match the user has open for review.

<sub>Verify level: **test**</sub>

---
