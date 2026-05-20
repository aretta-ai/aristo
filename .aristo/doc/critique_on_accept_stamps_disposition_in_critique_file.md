**Aristo verified intent — `critique_on_accept_stamps_disposition_in_critique_file`**

Accepting a critique finding stamps `disposition = accepted` into the corresponding finding inside `.aristo/critiques/<id>.critique`. Future `aristo critique --apply-findings` runs hide closed findings by default (the loop is closed: a reviewed finding doesn't re-surface). A refactor that updated the substrate's session state without touching the .critique file would leave the finding visible to every subsequent apply run as if no review had happened.

<sub>Verify level: **neural**</sub>

---
