**Aristo verified intent — `regenerate_index_rebuilds_in_memory_without_a_committed_index`**

`regenerate_index` rebuilds the full index in memory from source + `.aristo/proofs/` + `.aristo/canon-matches.toml`, with no dependency on a committed `.aristo/index.toml`. Same walk, cycle check, and binding derivation as `aristo stamp`, but status is sourced from proofs (`merge_status_from_proofs`), not carried from a prior index file. This is what lets the index become a gitignored local cache: any reader can call `load_index` and get correct, fresh status without the file existing.

<sub>Verify level: **neural**</sub>

---
