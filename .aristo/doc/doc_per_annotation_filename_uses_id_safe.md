**Aristo verified intent — `doc_per_annotation_filename_uses_id_safe`**

`aristo doc` writes each annotation to .aristo/doc/<id-safe>.md where `<id-safe>` substitutes `:` with `__`. Same convention as `.proof` and `.critique` files so users have one mental model for id↔filename mapping across the SDK. A regression that picks a different escape (or uses the raw id with `:`) would create platform-specific filename failures (`:` is illegal on Windows / macOS HFS+) AND silently break the slice-30 proc-macro that reads these files via `include_str!`.

<sub>Verify level: **neural**</sub>

---
