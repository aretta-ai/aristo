**Aristo verified intent — `extract_c_returns_annotations_in_source_order`**

C annotations return in source order — top of file first — mirroring the Rust extractor's contract. The parse tree is walked top-to-bottom and results are pushed in encounter order; collecting through any unordered structure, or sorting, would break the stable index ordering the downstream fixtures index into positionally.

<sub>Verify level: **test**</sub>

---
