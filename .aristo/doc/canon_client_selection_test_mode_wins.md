**Aristo verified intent — `canon_client_selection_test_mode_wins`**

Client selection order is load-bearing: ARISTO_CANON_FIXTURE wins outright (test mode beats everything, including auth), then auth-token resolution decides between HttpCanonClient and the free-tier Noop. Reversing — e.g. checking auth first — would make integration tests need a fake token to work, coupling test setup to the auth substrate unnecessarily.

<sub>Verify level: **test**</sub>

---
