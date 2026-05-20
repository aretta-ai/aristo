**Aristo verified intent — `walk_directory_is_deterministic`**

The same source tree yields byte-identical results across runs and machines: lexicographic path order, source order within each file. Parallelism or unsorted directory reads would silently break the index's reproducibility guarantee.

<sub>Verify level: **test**</sub>

---
