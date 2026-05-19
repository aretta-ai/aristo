# `aristo graph` — default Mermaid output to stdout

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Default — Mermaid to stdout".

Default format is Mermaid `flowchart TD` (top-down). No external dependency. Pastes directly into GitHub READMEs (renders inline) or any markdown viewer that supports Mermaid. Visual encoding: color = verify level (gray=`false`, yellow=`neural`, blue=`test`, green=`full`); shape = kind (rectangle=intent, hexagon=assume); border = red for critical status (stale/orphan/forged), default otherwise.

This scenario uses a minimal fixture of 3 annotations (2 intents + 1 assume + 1 parent edge + 1 stale-status critical border) so the output can be matched byte-for-byte.

````console
$ aristo graph
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

    %% Assume nodes (hexagons)
    storage_write_atomicity{{"storage_write_atomicity<br/>(assume)"}}:::vFalse

    %% Parent edges: child --> parent
    edit_page_writes_each_cell_once --> balance_no_duplicate_cells

    %% Critical-status border
    class edit_page_writes_each_cell_once critical
```
ok: 3 nodes, 1 edges rendered. (Mermaid to stdout)

````
