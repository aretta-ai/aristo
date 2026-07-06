**Aristo verified intent — `expose_prototype_is_verbatim_signature_not_reconstructed`**

The exposed prototype is the function's VERBATIM source signature (bytes up to the body brace), not a signature rebuilt from tree-sitter fields. A function carrying the `ARISTO_TU_LOCAL` macro prefix mis-parses into an ERROR node (the macro is read as the return type and the real return type as an error), so reconstructing `<type> <declarator>` from the tree would emit a WRONG prototype. Byte-range extraction is immune: the emitted `<signature>;` is exactly the real declaration, and `ARISTO_TU_LOCAL` expands to nothing when the harness compiles instrumented.

<sub>Verify level: **test**</sub>

---
