**Aristo verified intent — `c_site_selector_stays_out_of_shared_grammar`**

`site` is a C-only target selector and is peeled off here, never entering the shared AnnotationArgs grammar (design decision Option B). Adding site to AnnotationArgs to `simplify` this would pollute the Rust contract with a field Rust has no use for — Rust attaches structurally and never needs an explicit target. site must be the first argument so this peel stays a single leading-token check instead of a full re-parse of the arg list.

<sub>Verify level: **neural**</sub>

---
