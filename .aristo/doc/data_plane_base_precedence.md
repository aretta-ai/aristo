**Aristo verified intent — `data_plane_base_precedence`**

Data-plane base-URL precedence is exactly ARETTA_API_URL (env) > aristo.toml [instance] url > the account server, in that order and no other. A blank/whitespace env override is treated as unset (matching the login resolver, login_server) so it falls through to [instance] then server instead of routing to an empty base. A present env override is returned verbatim (trimmed, not normalized), preserving CI/test redirects; the [instance] url is normalized via ServerUrl::parse; server.as_str() is the final fallback (its default is code.aretta.ai). Reordering these tiers, dropping the blank-as-unset guard, or normalizing or dropping a present verbatim env override, would silently misroute verify and canon-match requests to the wrong Aretta deployment.

<sub>Verify level: **neural**</sub>

---
