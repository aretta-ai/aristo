//! §17 proof-tree suggestions — cross-repo contract decode test.
//!
//! `fixtures/canon-match.suggestions.golden.json` is a byte-copy of the
//! cross-repo contract at
//! `docs/mockups/17-proof-tree-suggestions/contract/canon-match.suggestions.golden.json`
//! (meta-workspace). toolsaurus (the producer) asserts it *emits* this
//! exact JSON; aristo (the consumer) asserts it *decodes* it into the
//! wire types with no loss. A field rename on either side breaks one of
//! the two tests — that is the anti-drift point.
//!
//! Scenario: one annotation matched the leaf `wal_commit_requires_fsync`
//! (P-014). The objective is populated here to exercise the full
//! post-Slice-0b shape (objective MAY be null pre-0b).

use aristo_core::canon::{
    CanonMatchResponse, ClusterSuggestion, PrefixTier, Relationship, SuggestedEntry,
};

const GOLDEN: &str = include_str!("fixtures/canon-match.suggestions.golden.json");

#[test]
fn golden_decodes_into_one_cluster_with_parent_objective_and_five_siblings() {
    let resp: CanonMatchResponse =
        serde_json::from_str(GOLDEN).expect("golden suggestions response must decode");

    // Primary results decode as before.
    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].len(), 1);
    assert_eq!(resp.results[0][0].canon_id, "wal_commit_requires_fsync");

    // Exactly one cluster, aligned by annotation index.
    let suggestions = resp
        .suggestions
        .expect("golden carries a suggestions field");
    assert_eq!(suggestions.len(), 1, "one cluster, aligned to results[]");
    let cluster: &ClusterSuggestion = suggestions[0]
        .as_ref()
        .expect("annotation 0 has a non-null cluster");
    assert_eq!(cluster.for_canon_id, "wal_commit_requires_fsync");

    // Objective = the kanon: proof-objective parent (D2).
    let objective: &SuggestedEntry = cluster
        .objective
        .as_ref()
        .expect("golden populates the objective (post-0b shape)");
    assert_eq!(objective.canon_id, "wal_protocol_correctness");
    assert_eq!(objective.prefix_tier, PrefixTier::Kanon);
    assert_eq!(objective.relationship, Relationship::Parent);
    assert!(
        objective.backed_by.is_none(),
        "kanon: objective has no backing"
    );

    // Five siblings, all relationship=sibling, all aristos: tier.
    assert_eq!(cluster.siblings.len(), 5, "five co-member siblings");
    assert!(
        cluster
            .siblings
            .iter()
            .all(|s| s.relationship == Relationship::Sibling),
        "every sibling carries relationship=sibling"
    );
    assert!(
        cluster
            .siblings
            .iter()
            .all(|s| s.prefix_tier == PrefixTier::Aristos),
        "every sibling is aristos: tier"
    );

    // The primary is excluded from siblings (server dedup ①).
    assert!(
        !cluster
            .siblings
            .iter()
            .any(|s| s.canon_id == "wal_commit_requires_fsync"),
        "the matched primary must not appear among its own siblings"
    );

    // Spot-check one sibling's full shape (carried verbatim).
    let s0 = &cluster.siblings[0];
    assert_eq!(s0.canon_id, "wal_nbackfills_orders_with_recovery");
    assert_eq!(s0.scope, "turso");
    assert_eq!(s0.version, "v0.1.0");
    assert_eq!(s0.verification.coverage_level, "partial");
    assert_eq!(s0.verification.test_binaries, vec!["wal_conform"]);
    assert_eq!(
        s0.backed_by.as_deref(),
        Some("golden model + proofs + differential testing")
    );
}

#[test]
fn golden_re_serializes_lossless_round_trip() {
    // Decode → re-encode → decode and assert structural stability. Pins
    // the field names + casing against silent drift.
    let resp: CanonMatchResponse = serde_json::from_str(GOLDEN).unwrap();
    let reencoded = serde_json::to_string(&resp).unwrap();
    let back: CanonMatchResponse = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(resp, back);
}
