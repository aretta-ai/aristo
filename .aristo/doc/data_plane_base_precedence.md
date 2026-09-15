**Aristo verified intent — `data_plane_base_precedence`**

Data-plane base-URL precedence is exactly ARETTA_API_URL (env) > the credential's server, and no other tier. A blank/whitespace env override is treated as unset (matching the login resolver, login_server) so it falls through to the credential's server instead of routing to an empty base; a present env override is normalized via ServerUrl::parse, the same reading every server spec gets. Adding a tier, dropping the blank-as-unset guard, or reading the env override differently from the login server would silently misroute verify and canon-match requests to the wrong Aretta deployment.

<sub>Verify level: **neural**</sub>

---
