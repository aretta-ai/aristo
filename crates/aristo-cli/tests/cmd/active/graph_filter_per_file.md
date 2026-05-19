# `aristo graph --filter file=<path>` — per-file scope

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Filtering — per-file".

`--filter file=<path>` restricts rendered nodes to annotations in the given file. The slice-29-MVP shape is exclusion-only; auto-inclusion of immediate external parents for context is a follow-up (the design doc's "+ immediate parents for context" pattern lands when the `--depth` flag in commit 7 grows beyond strict-match semantics).

````console
$ aristo graph --filter file=core/storage/btree.rs
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
