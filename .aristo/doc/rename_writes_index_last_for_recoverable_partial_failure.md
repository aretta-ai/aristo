**Aristo verified intent — `rename_writes_index_last_for_recoverable_partial_failure`**

Apply order is load-bearing: source files first (in any order; each is a single atomic temp+rename), THEN artifact moves, THEN the new index.toml LAST (atomic). The reason: if source writes complete but artifact-move or index-write fails, source has the new ids but the index still references the old ones. `aristo stamp` detects this and refuses with structural drift — the user reverts or completes manually. The reverse order (index first, source last) would leave the user with an index pointing at ids the source doesn't define, making `aristo show` / `aristo list` lie. No real transactional rollback ships in slice 32 (out-of-scope per HANDOFF); 'best-effort recoverable' is the contract.

<sub>Verify level: **test**</sub>

---
