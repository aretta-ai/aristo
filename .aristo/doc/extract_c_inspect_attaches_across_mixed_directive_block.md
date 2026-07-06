**Aristo verified intent — `extract_c_inspect_attaches_across_mixed_directive_block`**

inspect directives attach to the type on the line directly below a contiguous block of `// @aristo` directive lines — an intervening intent/assume directive does NOT break the block (adjacency is measured from the last aristo directive of any kind), but a plain comment or a blank-line gap does. This keeps a struct's intent and its inspect directives freely interleavable above it while a reformatter that inserts a blank line still (correctly) detaches them.

<sub>Verify level: **test**</sub>

---
