**Aristo verified intent — `stamp_derives_canon_binding_from_cache`**

Canon binding state is derived from `.aristo/canon-matches.toml`, not preserved across stamp runs. For every index entry whose id carries a canon prefix (`aristos:` / `kanon:`) and has a matching row in the cache's `accepted_matches`, set the binding to `BindingState::Bound { linked }` (or `AssumeEntry::linked = Some(...)`). If the cache row's `linked` field is absent — older caches written before the field was added, or Phase 1 carve-outs where the server omits it — synthesize a deterministic placeholder from `(canon_id, version)`, identical to what `canon accept` would have written. Source has a canon prefix but no cache row → leave Local and emit a diagnostic; the binding was orphaned (cache deleted or never fetched) and the user must re-run with `--refresh-canon` or re-accept.

<sub>Verify level: **test**</sub>

---
