**Aristo verified intent — `pending_neural_file_is_sdk_to_agent_request_not_a_result`**

Each pending verify task is a request from the SDK to the in-agent skill, not a result the SDK reads back. The SDK writes it to the verify queue and consumes verdicts only through the submit path, after the validator gates them. A refactor that has the SDK process its own queue directly would erase the CLI/agent split the queue exists to enforce.

<sub>Verify level: **neural**</sub>

---
