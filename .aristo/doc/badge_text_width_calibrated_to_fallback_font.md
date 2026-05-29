**Aristo verified intent — `badge_text_width_calibrated_to_fallback_font`**

Badge text width is approximated per-character, scaled by font size, and deliberately calibrated to the WIDER fallback sans (DejaVu/Verdana) rather than the narrower brand font (Fira Sans Condensed). GitHub strips the web-font fetch from committed SVGs, so the README badge renders in the fallback; sizing to the brand font's metrics would clip the tier text there. Over-estimating is safe (a little right padding); under-estimating clips. The trycmd scenarios match the SVG with wildcards (only `<svg ...>` ↔ `</svg>` framing is pinned, not pixel dimensions), so this heuristic is the sole guard against clipping.

<sub>Verify level: **neural**</sub>

---
