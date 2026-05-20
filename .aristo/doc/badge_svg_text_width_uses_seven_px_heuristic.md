**Aristo verified intent — `badge_svg_text_width_uses_seven_px_heuristic`**

SVG text width is approximated as 7px per character in the badge body and 10px padding on each end. This deviates slightly from shields.io's per-glyph metrics table (DejaVu Sans), but the trycmd scenarios match the SVG with byte-level wildcards (the spec only pins `<svg ...>` ↔ `</svg>` framing, not exact pixel dimensions). A regression that broke the 7px/10px convention without updating downstream consumers (rendering pipelines that pin widths) would produce misaligned text rendering at the edges.

<sub>Verify level: **neural**</sub>

---
