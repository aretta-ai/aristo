**Aristo verified intent — `count_fns_processes_rust_files_only`**

The fn-counter parses each file as Rust with `syn`, so it processes ONLY `.rs` files — even though `walk_for_freshness_with` also returns `.c`/`.h` (which it must, for the freshness preflight to drift-check C source). Feeding a `.c` file into the Rust parser here is a hard error, so tier/metrics computation would blow up on any mixed Rust+C repo. The `.rs` filter must stay until this counter learns to count C functions too; widening the walk without re-narrowing here re-breaks it.

<sub>Verify level: **test**</sub>

---
