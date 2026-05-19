## Aristo verified annotations

This crate carries **3 Aristo annotations** (2 intent · 1 assume).

| Verify level | Count |
|---|---|
| `false` (documentation only) | 0 |
| `"neural"` | 0 |
| `"test"` | 0 |
| `"full"` | 2 |

**0 annotations are server-bound** (`aristos:` namespace) and verified by the
Aristo proof system. See the [annotation graph](./_graph.svg) for the full
property structure.

---

## Annotation graph

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
