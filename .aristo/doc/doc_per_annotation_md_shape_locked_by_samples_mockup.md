**Aristo verified intent — `doc_per_annotation_md_shape_locked_by_samples_mockup`**

Per-annotation markdown structure is locked by the I1 `samples.md` mockup: header line (`**Aristo verified intent — \`<id>\`**` for intents, `**Aristo assumption — \`<id>\`**` for assumes), blank line, body text verbatim, blank line, `<sub>` metadata line, blank line, `---`. The metadata line composes verify-level + server-bound marker + parent link with ` · ` separators. A regression that drops the trailing `---` would break readers that include this MD with `include_str!` between other doc blocks — the separator is what isolates this annotation from surrounding rustdoc.

<sub>Verify level: **neural**</sub>

---
