**Aristo verified intent — `badge_stdout_mode_keeps_svg_uncorrupted`**

When `--out` is omitted, the SVG goes to stdout and ALL progress / advisory output goes to stderr — never to stdout. A regression that emitted a progress line to stdout in this mode would corrupt the SVG, breaking any consumer that pipes `aristo badge > foo.svg`. Every diagnostic (warnings, hints) MUST stay on stderr for the no-`--out` path.

<sub>Verify level: **neural**</sub>

---
