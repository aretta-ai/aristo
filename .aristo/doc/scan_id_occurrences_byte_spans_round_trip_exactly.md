**Aristo verified intent — `scan_id_occurrences_byte_spans_round_trip_exactly`**

scan_id_occurrences returns byte-range spans that splice exactly the id value when used as `source[byte_start..byte_end]`. Slice 32's rename command rewrites source by byte substitution at these spans rather than by syn::visit_mut re-serialization — re-serialization destroys whitespace + comments and produces user-visible churn. Spans MUST exclude surrounding quotes so the new id can be spliced in verbatim.

<sub>Verify level: **test**</sub>

---
