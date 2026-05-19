# `aristo graph --filter status=<state>` — status-filtered visualization

Source: `../aretta-sdk/docs/mockups/11-gap-closures/cli-sessions.md` § "J3 — `aristo graph --filter status=...`".

J3 added `status=<state>` to the unified filter grammar's surfacing on `aristo graph`. Useful for review-meeting questions like "show me what's still unverified" or "show me only the stale ones".

````console
$ aristo graph --filter status=stale
? 0
```mermaid
flowchart TD
    classDef vFalse  fill:#e5e5e5,stroke:#999
    classDef vNeural fill:#fef3c7,stroke:#b45309
    classDef vTest   fill:#dbeafe,stroke:#1d4ed8
    classDef vFull   fill:#bbf7d0,stroke:#15803d
    classDef critical stroke:#dc2626,stroke-width:3px

    %% Intent nodes (rectangles)
    edit_page_writes_each_cell_once["edit_page_writes_each_cell_once<br/>(intent, verify=full)"]:::vFull

    %% Critical-status border
    class edit_page_writes_each_cell_once critical
```
ok: 1 nodes, 0 edges rendered. (Mermaid to stdout)

````
