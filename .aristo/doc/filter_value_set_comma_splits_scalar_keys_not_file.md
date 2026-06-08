**Aristo verified intent — `filter_value_set_comma_splits_scalar_keys_not_file`**

`id`, `parent`, and `status` values split on `,` into a value-level OR set (`id=a,b` matches a OR b); members are trimmed and empties dropped, and an all-empty value (`id=,`) is `EmptyValue`. `file` is deliberately NOT comma-split — its optional `:<LO>-<HI>` range suffix and the fact that a path may contain a `,` make splitting ambiguous. A refactor that routed `file` through this helper "for consistency" would silently break range parsing and comma-bearing paths.

<sub>Verify level: **test**</sub>

---
