**Aristo verified intent — `rust_covered_regions_preserve_source_bytes`**

Rust covered-region text preserves the original source bytes, including newline encodings and Unicode. Parser spans refer to the same buffer that is sliced; source text is never reconstructed from lines or tokens.

<sub>Verify level: **test**</sub>

---
