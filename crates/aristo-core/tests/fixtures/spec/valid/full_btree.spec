# Comprehensive sample drawn from
# ../../../../../../../../../aretta-sdk/docs/mockups/04-staleness/sample.spec
# (Fresh state — the §4 mockup also documents the Stale state in commented
# form; the stale.spec fixture exercises that path.)

annotation_id        = "insert_into_cell_postcondition"
annotation_text_hash = "sha256:3a7c9e1b5f2d4068c91e7b2a8d9c3f4e7a2b5c8d1e4f7a9b2c5e8d1f4a7c9e1b"
source_body_hash     = "sha256:8d1f4a7c9e1b5f2d4068c91e7b2a8d9c3f4e7a2b5c8d1e4f7a9b2c5e8d1f4a7c"
covered_region       = "function"
covered_region_path  = "src/btree.rs::insert_into_cell"
mined_at             = "2026-05-13T14:23:00Z"
mined_by             = "aristo verify (skill=aristo-mine-assertions, host=claude-code, model=off-the-shelf)"
human_reviewed       = false
notes                = ""
---
# The Rust assertion below is what gets injected when the
# `aristo_verify` cargo feature is enabled. Default builds expand
# the macro to nothing; verify-mode builds inject this body.

{
    let prior_count = contents.cell_count();
    let snapshot: Vec<(usize, Vec<u8>)> = (0..prior_count)
        .map(|i| (i, contents.cell_at(i).to_vec()))
        .collect();

    // … original function body runs …

    debug_assert_eq!(
        contents.cell_count(),
        prior_count + 1,
        "annotation insert_into_cell_postcondition violated: \
         cell_count did not increase by exactly 1"
    );

    let inserted = contents.cell_at(cell_idx);
    debug_assert_eq!(
        inserted, payload,
        "annotation insert_into_cell_postcondition violated: \
         new cell does not occupy the requested index"
    );

    for (orig_idx, orig_bytes) in &snapshot {
        let new_idx = if *orig_idx < cell_idx { *orig_idx } else { *orig_idx + 1 };
        debug_assert_eq!(
            contents.cell_at(new_idx),
            &orig_bytes[..],
            "annotation insert_into_cell_postcondition violated: \
             pre-existing cell {} was not preserved under the shift",
            orig_idx
        );
    }
}
