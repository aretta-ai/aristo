**Aristo verified intent — `verify_migrates_legacy_pending_neural_toml_to_queue`**

If a legacy `.aristo/pending-neural.toml` (single-file format from v0.0.6) is present, expand each entry into per-id queue files under `.aristo/verify-queue/pending/` and delete the legacy file. Runs at the start of every `aristo verify` invocation. Idempotent: a second run with no legacy file is a no-op. Single-pass migration — there is no compat shim that re-reads the legacy format on subsequent runs.

<sub>Verify level: **test**</sub>

---
