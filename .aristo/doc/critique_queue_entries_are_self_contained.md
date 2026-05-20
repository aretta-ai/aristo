**Aristo verified intent — `critique_queue_entries_are_self_contained`**

Critique queue entries embed the focal annotation text PLUS sibling and parent annotation texts as a self-contained TOML body under `.aristo/critique-queue/pending/<id>.toml`. Workers get Bash-only tooling (no Read, no Write) and decide findings purely from the embedded context — they cannot wander into the repo. A refactor that left the queue entry thin (id + hash only, agent reads source itself) would re-introduce the very failure mode this design defends against: agents spending tokens on unrelated reads and producing critique grounded in irrelevant code.

<sub>Verify level: **neural**</sub>

---
