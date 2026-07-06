**Aristo verified intent — `submit_returns_sha256_of_written_file`**

On accept, the SDK prints `accepted: sha256:<hex>` to stdout, where <hex> is the sha256 of the TOML body that landed on disk. Because the SDK is the sole writer, the orchestrator can hash the text its subagent reported and compare it: a mismatch means the subagent's text diverged from what was written. This anchors the write-acknowledgement, so the orchestrator never has to re-read the file to trust the subagent's report.

<sub>Verify level: **neural**</sub>

---
