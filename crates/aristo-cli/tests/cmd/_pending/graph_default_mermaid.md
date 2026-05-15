# `aristo graph` — default Mermaid output to stdout

Source: `../aretta-sdk/docs/mockups/10-doc-and-graph/cli-sessions.md` § "I2 → Default — Mermaid to stdout".

Default format is Mermaid `flowchart TD` (top-down). No external dependency. Pastes directly into GitHub READMEs (renders inline) or any markdown viewer that supports Mermaid. Visual encoding: color = verify level (gray=`false`, yellow=`neural`, blue=`test`, green=`full`); shape = kind (rectangle=intent, hexagon=assume); border = red for critical status (stale/orphan/forged), default otherwise.

```console
$ aristo graph

```mermaid
flowchart TD
    classDef vFalse  fill:#e5e5e5,stroke:#999
    classDef vNeural fill:#fef3c7,stroke:#b45309
    classDef vTest   fill:#dbeafe,stroke:#1d4ed8
    classDef vFull   fill:#bbf7d0,stroke:#15803d
    classDef critical stroke:#dc2626,stroke-width:3px

    A1["aristos:balance_no_duplicate_cells<br/>(intent, verify=full)"]:::vFull
[..]
```

ok: [..] nodes, [..] edges rendered. (Mermaid to stdout)
```
