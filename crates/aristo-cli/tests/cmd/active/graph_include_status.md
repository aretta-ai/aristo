# `aristo graph --include-status` — color-by-status

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → `--include-status` — status-colored".

`--include-status` swaps the color axis from verify level to current B5b status. Verify level moves to an in-node label. Use when the dominant question is "what's still unverified" (review meetings, audit prep). Default mode (verify-level coloring) is for documentation use (READMEs, rustdoc).

The status palette differentiates severity directly (red border on sForged / sCounterexample / sInconclusive), so the verify-mode "critical" overlay isn't stacked on top.

````console
$ aristo graph --include-status
? 0
```mermaid
flowchart TD
    classDef sVerified       fill:#bbf7d0,stroke:#15803d
    classDef sTested         fill:#dbeafe,stroke:#1d4ed8
    classDef sNeural         fill:#fef3c7,stroke:#b45309
    classDef sStale          fill:#fed7aa,stroke:#c2410c
    classDef sOrphan         fill:#e9d5ff,stroke:#7e22ce
    classDef sForged         fill:#fecaca,stroke:#dc2626,stroke-width:3px
    classDef sUnknown        fill:#e5e5e5,stroke:#9ca3af
    classDef sPendingDeepen  fill:#e5e5e5,stroke:#9ca3af
    classDef sCounterexample fill:#fecaca,stroke:#dc2626,stroke-width:3px
    classDef sInconclusive   fill:#fed7aa,stroke:#c2410c,stroke-width:3px

    %% Intent nodes (rectangles)
    balance_no_duplicate_cells["balance_no_duplicate_cells<br/>(intent, verify=full)"]:::sVerified
    edit_page_writes_each_cell_once["edit_page_writes_each_cell_once<br/>(intent, verify=full)"]:::sStale

    %% Assume nodes (hexagons)
    storage_write_atomicity{{"storage_write_atomicity<br/>(assume)"}}:::sUnknown

    %% Parent edges: child --> parent
    edit_page_writes_each_cell_once --> balance_no_duplicate_cells
```
ok: 3 nodes, 1 edges rendered. (Mermaid to stdout)

````
