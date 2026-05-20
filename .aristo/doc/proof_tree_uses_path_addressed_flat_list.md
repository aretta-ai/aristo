**Aristo verified intent — `proof_tree_uses_path_addressed_flat_list`**

Proof steps are a flat list indexed by dotted-path strings ("0", "0.0", "0.1.2"), not a recursive Rust struct. TOML doesn't serialize recursive heterogeneous structures cleanly, and the path encoding lets the validator and the promotion flow reference any node by stable string. A recursive-enum refactor breaks both.

<sub>Verify level: **neural**</sub>

---
