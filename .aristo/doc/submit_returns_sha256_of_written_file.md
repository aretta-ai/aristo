**Aristo verified intent — `submit_returns_sha256_of_written_file`**

On accept, the SDK prints `accepted: sha256:<hex>` to stdout where <hex> is the sha256 of the on-disk TOML body. The orchestrator can compare this against body_hash(text_returned_by_subagent) for a cheap integrity check: SDK is the sole writer, so a mismatch means the subagent's reported text diverged from what hit disk (corrupted cache, fabricated response). The hash anchors the write-acknowledgement so the orchestrator does not have to re-read the file to validate the subagent's word.

<sub>Verify level: **neural**</sub>

---
