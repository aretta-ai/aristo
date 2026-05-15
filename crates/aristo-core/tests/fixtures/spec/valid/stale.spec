annotation_id        = "drifted_postcondition"
annotation_text_hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222"
source_body_hash     = "sha256:3333333333333333333333333333333333333333333333333333333333333333"
covered_region       = "function"
covered_region_path  = "src/lib.rs::drifted"
mined_at             = "2026-05-13T14:23:00Z"
mined_by             = "aristo verify (skill=aristo-mine-assertions)"
human_reviewed       = false
notes                = ""
stale_at             = "2026-05-15T09:14:22Z"
current_body_hash    = "sha256:4444444444444444444444444444444444444444444444444444444444444444"
---
{
    // Body the assertion was mined against — preserved verbatim by stamp's
    // staleness detection. The next `aristo verify` re-mines.
    debug_assert!(true);
}
