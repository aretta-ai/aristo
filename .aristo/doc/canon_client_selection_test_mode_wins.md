**Aristo verified intent — `canon_client_selection_test_mode_wins`**

Client selection order is load-bearing: ARISTO_CANON_FIXTURE wins outright (test mode beats everything, including auth), then auth-token resolution decides between HttpCanonClient, the free-tier outcome (only AuthError::NoToken — nothing on file) and the unresolved outcome (credentials on file but unusable for this checkout, or malformed). Reversing — e.g. checking auth first — would make integration tests need a fake token to work, coupling test setup to the auth substrate unnecessarily; collapsing unresolved into free-tier would tell a signed-in user to start a trial.

<sub>Verify level: **test**</sub>

---
