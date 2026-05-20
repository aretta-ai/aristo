**Aristo verified intent — `pending_neural_file_is_sdk_to_agent_request_not_a_result`**

Each pending verify task is a REQUEST from the SDK to the in-agent skill, enqueued at `.aristo/verify-queue/pending/<id>.toml`. Workers pop one at a time via `aristo verify --pop-next`. The SDK never reads these task files back as verdicts — verdicts arrive via `aristo verify --submit-verdict` and land at `.aristo/proofs/<id>.proof` after the mechanical validator gates them. A refactor that has the SDK auto-process its own queue (e.g., to call an LLM directly) would conflate the CLI with the agent and break the design split: the CLI never makes LLM calls; the agent never bypasses the SDK validator.

<sub>Verify level: **neural**</sub>

---
