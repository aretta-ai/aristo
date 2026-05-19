# `aristo graph` — composed filters AND together

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Combined filters".

Multiple `--filter` flags AND together (per the J2 unified grammar). All annotations must match ALL filters to be rendered.

````console
$ aristo graph --filter file=core/storage/btree.rs --filter status=verified
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
```
ok: 1 nodes, 0 edges rendered. (Mermaid to stdout)

````
