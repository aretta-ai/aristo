**Aristo verified intent — `data_plane_base_precedence`**

Data-plane base-URL precedence is exactly ARETTA_API_URL (env) > aristo.toml [instance] url > the account server, in that order and no other. The env override is returned verbatim (preserving the prior override behavior and CI/test redirects); the [instance] url is normalized via ServerUrl::parse; server.as_str() is the final fallback (its default is code.aretta.ai). Reordering these tiers, or normalizing or dropping the verbatim env override, would silently misroute verify and canon-match requests to the wrong Aretta deployment.

<sub>Verify level: **neural**</sub>

---
