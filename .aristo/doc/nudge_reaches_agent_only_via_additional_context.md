**Aristo verified intent — `nudge_reaches_agent_only_via_additional_context`**

A hook reaches the agent's context ONLY through `hookSpecificOutput.additionalContext`; a plain string (even a literal `<system-reminder>`) printed to a hook's stdout lands in the transcript but never in the model's context. Every agent-facing nudge MUST use this JSON envelope — emitting bare text produces a nudge that is transmitted but never received by the model.

<sub>Verify level: **test**</sub>

---
