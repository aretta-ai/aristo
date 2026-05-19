# `aristo graph --exclude-assumes` — focus on the intent graph

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → `--exclude-assumes` — focus on intent graph".

Hides `assume` nodes from the rendered graph. Default is to include them — assumes form the "ground" of the property graph and dropping them by default would hide load-bearing background facts. Useful when the question is "what's the verified-property structure" and the OS / library assumptions are noise.

````console
$ aristo graph --exclude-assumes
? 0
```mermaid
flowchart TD
    classDef vFalse  fill:#e5e5e5,stroke:#999
    classDef vNeural fill:#fef3c7,stroke:#b45309
    classDef vTest   fill:#dbeafe,stroke:#1d4ed8
    classDef vFull   fill:#bbf7d0,stroke:#15803d
    classDef critical stroke:#dc2626,stroke-width:3px

    %% Intent nodes (rectangles)
    balance_no_duplicate_cells["balance_no_duplicate_cells<br/>(intent, verify=full)"]:::vFull
    edit_page_writes_each_cell_once["edit_page_writes_each_cell_once<br/>(intent, verify=full)"]:::vFull

    %% Parent edges: child --> parent
    edit_page_writes_each_cell_once --> balance_no_duplicate_cells

    %% Critical-status border
    class edit_page_writes_each_cell_once critical
```
ok: 2 nodes, 1 edges rendered. (Mermaid to stdout)

````
