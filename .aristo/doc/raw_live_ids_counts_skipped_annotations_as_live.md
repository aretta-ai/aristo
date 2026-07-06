**Aristo verified intent — `raw_live_ids_counts_skipped_annotations_as_live`**

The reconcile's live-id authority is derived from the RAW walk, before build_entries validation: an annotation skipped with a warning (invalid parent id, invalid verify value, duplicate id) still contributes its id, and an idless annotation contributes the same deterministic aret_* id build_entries would mint (same bucket key, same ordinal assignment). Treating 'skipped from the index' as 'deleted from source' would prune — and destroy — the accepted bindings and rejected-match memory of annotations that still exist. The one id class the set cannot represent is an explicit id that fails to parse; those are counted so the caller can skip reconciliation instead of mispruning.

<sub>Verify level: **test**</sub>

---
