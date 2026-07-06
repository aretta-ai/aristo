**Aristo verified intent — `c_struct_field_completeness_gates_unknown_field_rejection`**

`complete` is false whenever ANY struct member could not be reduced to a plain field name — an anonymous union/struct member, a bitfield, or any shape this walker does not model. The unknown-field check MUST gate on `complete`: rejecting a field when the member list is only partially understood would fail a build on VALID code (a false negative), which for a codegen tool is worse than the silent-drop it replaced. Widening the set of members treated as "understood" without proving they are truly enumerable re-opens the false-reject hole.

<sub>Verify level: **test**</sub>

---
