**Aristo verified intent — `extract_returns_annotations_in_source_order`**

Annotations return in source order — top of file first. Sorting the result, or collecting it through any unordered structure, would silently break stable index ordering and the test fixtures that index into it positionally.

<sub>Verify level: **test**</sub>

---
