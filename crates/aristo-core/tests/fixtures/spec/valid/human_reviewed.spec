annotation_id        = "carefully_reviewed_postcondition"
annotation_text_hash = "sha256:5555555555555555555555555555555555555555555555555555555555555555"
source_body_hash     = "sha256:6666666666666666666666666666666666666666666666666666666666666666"
covered_region       = "function"
covered_region_path  = "src/lib.rs::critical"
mined_at             = "2026-05-13T14:23:00Z"
mined_by             = "aristo verify (skill=aristo-mine-assertions)"
human_reviewed       = true
notes                = "Manually verified by alice; do not silently overwrite on staleness."
---
{
    // human_reviewed=true means stamp won't overwrite this on staleness;
    // a `.candidate` file is produced instead for human diff review.
    debug_assert!(post_condition_holds());
}
