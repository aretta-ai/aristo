//! Coverage-integrity check (SLICE23-SPEC aristo item 6) — PURE.
//!
//! The "keyless gated row" condition surfaced client-side: an
//! accepted match whose verification metadata routes test binaries
//! but carries no (or an empty) instrumentation bundle. If any of
//! those tests are instrumentation-gated, they cannot compile against
//! an uninstrumented SUT and the coverage the match advertises is
//! hollow. Gating itself is server-side knowledge (coverage.yaml
//! `feature_gates`) that the wire does not carry, so this is a
//! warning, not an error — phrased conditionally.
//!
//! Pre-P-008 accepted matches (no `verification` at all) are skipped:
//! there is nothing to inspect and nothing was promised.

use aristo_core::canon::CanonMatchesFile;

/// One warning line per accepted match that names test binaries but
/// delivers no accessor records.
pub(crate) fn coverage_integrity_warnings(cache: &CanonMatchesFile) -> Vec<String> {
    let mut warnings = Vec::new();
    for (annotation_id, entry) in &cache.entries {
        for accepted in &entry.accepted_matches {
            let Some(vm) = &accepted.verification else {
                continue;
            };
            if vm.test_binaries.is_empty() {
                continue;
            }
            let hole = match &vm.instrumentation {
                None => Some("no instrumentation bundle"),
                Some(b) if b.records.is_empty() => {
                    Some("an empty instrumentation bundle (0 records)")
                }
                Some(_) => None,
            };
            if let Some(what) = hole {
                warnings.push(format!(
                    "coverage-integrity: accepted match `{}` on `{}` routes test \
                     binaries [{}] but carries {what} — if any of those tests are \
                     instrumentation-gated they cannot compile against an \
                     uninstrumented SUT (keyless gated row: the server delivered \
                     no accessor records)",
                    accepted.canon_id,
                    annotation_id.as_str(),
                    vm.test_binaries.join(", ")
                ));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::canon::{
        AcceptedMatch, CacheEntry, InstrumentationBundle, PrefixTier, VerificationMetadata,
    };
    use aristo_core::index::AnnotationId;

    const GOLDEN: &str = include_str!("fixtures/golden-bundle.json");

    fn accepted(verification: Option<VerificationMetadata>) -> AcceptedMatch {
        AcceptedMatch {
            canon_id: "wal_install_coherence".into(),
            version: "v0.1.0".into(),
            canonical_text: "text".into(),
            canon_version: "v0.2.0".into(),
            confidence: 1.0,
            prefix_tier: PrefixTier::Aristos,
            backed_by: None,
            linked: None,
            verification,
            accepted_at: "2026-07-02T00:00:00Z".into(),
            bound_at: "2026-07-02T00:00:00Z".into(),
        }
    }

    fn cache_with(matches: Vec<AcceptedMatch>) -> CanonMatchesFile {
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            AnnotationId::parse("aristos:wal_install_invariant").unwrap(),
            CacheEntry {
                last_match_text_hash: "blake3:x".into(),
                canon_fetched_at: "2026-07-02T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: matches,
                rejected_matches: vec![],
            },
        );
        cache
    }

    fn vm(
        test_binaries: Vec<String>,
        bundle: Option<InstrumentationBundle>,
    ) -> VerificationMetadata {
        VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries,
            instrumentation: bundle,
        }
    }

    #[test]
    fn gated_tests_without_bundle_warn() {
        let cache = cache_with(vec![accepted(Some(vm(
            vec!["wr03_install_coherence".into()],
            None,
        )))]);
        let warnings = coverage_integrity_warnings(&cache);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        let w = &warnings[0];
        assert!(w.contains("coverage-integrity"), "got: {w}");
        assert!(w.contains("wal_install_coherence"), "names the match: {w}");
        assert!(w.contains("wr03_install_coherence"), "names the tests: {w}");
        assert!(w.contains("no instrumentation bundle"), "got: {w}");
    }

    #[test]
    fn empty_bundle_records_also_warn() {
        let mut bundle: InstrumentationBundle = serde_json::from_str(GOLDEN).unwrap();
        bundle.records.clear();
        let cache = cache_with(vec![accepted(Some(vm(
            vec!["wr03_install_coherence".into()],
            Some(bundle),
        )))]);
        let warnings = coverage_integrity_warnings(&cache);
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(
            warnings[0].contains("empty instrumentation bundle"),
            "got: {}",
            warnings[0]
        );
    }

    #[test]
    fn populated_bundle_and_pre_p008_matches_stay_quiet() {
        let bundle: InstrumentationBundle = serde_json::from_str(GOLDEN).unwrap();
        let cache = cache_with(vec![
            // Healthy: routed tests + delivered bundle.
            accepted(Some(vm(
                vec!["wr03_install_coherence".into()],
                Some(bundle),
            ))),
            // No routed tests: nothing promised.
            accepted(Some(vm(vec![], None))),
            // Pre-P-008 row: nothing to inspect.
            accepted(None),
        ]);
        assert!(coverage_integrity_warnings(&cache).is_empty());
    }
}
