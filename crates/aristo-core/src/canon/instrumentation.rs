//! P-008 instrumentation-bundle helpers: union across accepted
//! suggestions + persistence sanitizing.
//!
//! Provenance: SLICE23-SPEC aristo item 3 (aretta-books
//! `.planning/instrument-handoff/slice23/SLICE23-SPEC.md`). The
//! server attaches an [`InstrumentationBundle`] to each `/canon/match`
//! suggestion's verification metadata; the SDK carries it through
//! `PendingMatch` → `AcceptedMatch` (`.aristo/canon-matches.toml`).
//! Before the S2 presence probe compiles anything, the client unions
//! the bundles of every *accepted* match into one work list.
//!
//! ## Why the union walks `accepted_matches` (not the suggestions queue)
//!
//! §17 cluster suggestions also carry verification metadata on the
//! wire, but adoption of a suggested sibling funnels through "write
//! `#[aristo::intent]` → stamp → canon accept" (see
//! `session_kind.rs` D4) — so by the time an accessor is actually
//! *required*, its bundle has arrived on a pending match and moved to
//! `accepted_matches` at accept time. The queue's `SuggestedMatch`
//! deliberately does not duplicate the bundle.
//!
//! ## Union semantics (per the spec)
//!
//! - Records dedup by `accessor_id`; companions dedup by
//!   `(symbol, file)` — first occurrence wins, input order preserved.
//! - Provenance must agree on `payload_ref`. On mismatch the union
//!   does NOT merge across revisions: it keeps one group per
//!   `payload_ref` (each independently unioned) and returns a
//!   warning for the caller to print. Mixing accessors authored
//!   against different payload revisions into one probe compile would
//!   blame the wrong revision when a symbol is missing.

use std::collections::{BTreeMap, BTreeSet};

use super::cache::CanonMatchesFile;
use super::types::InstrumentationBundle;

/// Result of unioning instrumentation bundles: one merged bundle per
/// `provenance.payload_ref` group (usually exactly one), plus
/// warnings for the caller to surface. Core stays print-free — the
/// CLI decides how to render the warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BundleUnion {
    /// One unioned bundle per `payload_ref` group, ordered by
    /// `payload_ref` (deterministic regardless of input order).
    pub bundles: Vec<InstrumentationBundle>,
    /// Human-readable warnings (provenance disagreements). Empty in
    /// the healthy single-revision case.
    pub warnings: Vec<String>,
}

/// Union instrumentation bundles across accepted suggestions.
///
/// Records dedup by `accessor_id`, companions by `(symbol, file)`.
/// Bundles whose provenance disagrees on `payload_ref` are kept as
/// separate per-`payload_ref` groups with a warning (never merged —
/// see the module docs). Empty input yields an empty union.
pub fn union_bundles<'a, I>(bundles: I) -> BundleUnion
where
    I: IntoIterator<Item = &'a InstrumentationBundle>,
{
    // BTreeMap keying gives deterministic group order by payload_ref;
    // the Vec preserves input order within a group (first-seen wins
    // on dedup below).
    let mut groups: BTreeMap<&str, Vec<&InstrumentationBundle>> = BTreeMap::new();
    for b in bundles {
        groups
            .entry(b.provenance.payload_ref.as_str())
            .or_default()
            .push(b);
    }

    let mut warnings = Vec::new();
    if groups.len() > 1 {
        let refs: Vec<&str> = groups.keys().copied().collect();
        warnings.push(format!(
            "instrumentation bundles disagree on provenance payload_ref ({}); \
             keeping {} separate per-payload_ref groups instead of merging",
            refs.join(", "),
            groups.len()
        ));
    }

    let merged = groups
        .values()
        .map(|group| union_group(group, &mut warnings))
        .collect();

    BundleUnion {
        bundles: merged,
        warnings,
    }
}

/// Union the bundles persisted on the cache's `accepted_matches`
/// buckets — the SDK-side entry point for "everything my accepted
/// canon bindings require". Pending (unreviewed) and rejected matches
/// are deliberately excluded: the probe should only compile-check
/// accessors the user has actually committed to.
pub fn union_accepted_bundles(cache: &CanonMatchesFile) -> BundleUnion {
    union_bundles(
        cache
            .entries
            .values()
            .flat_map(|entry| entry.accepted_matches.iter())
            .filter_map(|accepted| accepted.verification.as_ref())
            .filter_map(|vm| vm.instrumentation.as_ref()),
    )
}

/// Union one same-`payload_ref` group of bundles.
fn union_group(
    group: &[&InstrumentationBundle],
    warnings: &mut Vec<String>,
) -> InstrumentationBundle {
    let first = group[0];

    // Records dedup by accessor_id; companions by (symbol, file).
    // First occurrence wins, input order preserved — the server
    // renders lock-file order, which is worth keeping stable.
    let mut records = Vec::new();
    let mut seen_records: BTreeSet<&str> = BTreeSet::new();
    let mut companions = Vec::new();
    let mut seen_companions: BTreeSet<(&str, &str)> = BTreeSet::new();
    for b in group {
        for r in &b.records {
            if seen_records.insert(r.accessor_id.as_str()) {
                records.push(r.clone());
            }
        }
        for c in &b.companions {
            if seen_companions.insert((c.symbol.as_str(), c.file.as_str())) {
                companions.push(c.clone());
            }
        }
        // Same payload_ref but diverging sibling provenance fields
        // (base_ref / grammar rev / sut_binding) is a server-side
        // inconsistency worth surfacing — we keep `first`'s block.
        if b.provenance != first.provenance {
            warnings.push(format!(
                "instrumentation bundles for payload_ref {} disagree on other \
                 provenance fields (base_ref/macro_grammar_rev/sut_binding); \
                 keeping the first bundle's provenance block",
                first.provenance.payload_ref
            ));
        }
    }

    // bundle_id: when some input bundle already covers exactly the
    // unioned accessor set (the common case — every accepted
    // suggestion carried the same server-rendered bundle), keep that
    // server-issued id VERBATIM. Only a genuine merge of differing
    // record sets rederives an id client-side.
    let bundle_id = group
        .iter()
        .find(|b| {
            let ids: BTreeSet<&str> = b.records.iter().map(|r| r.accessor_id.as_str()).collect();
            ids == seen_records
        })
        .map(|b| b.bundle_id.clone())
        .unwrap_or_else(|| {
            rederive_bundle_id(
                &first.bundle_id,
                &first.provenance.payload_ref,
                &seen_records,
            )
        });

    InstrumentationBundle {
        bundle_id,
        provenance: first.provenance.clone(),
        compile_check: first.compile_check.clone(),
        companions,
        records,
    }
}

/// Client-side rederivation of the bundle-id rule
/// (`"<flavor>:<payload_ref[..7]>:<sha256(sorted accessor_ids joined ',')[..12]>"`,
/// SLICE23-SPEC) for a *merged* record set that matches no single
/// server-rendered bundle. The flavor prefix is reused from an input
/// bundle's id; the hash is sha256 over the sorted accessor ids joined
/// with `,` — the same join byte the conductor's server-side renderer
/// uses, so a client-rederived id for a record set equals what the
/// server would issue for it (the union still keeps a server-issued id
/// verbatim whenever one covers the set).
fn rederive_bundle_id(template_id: &str, payload_ref: &str, sorted_ids: &BTreeSet<&str>) -> String {
    use sha2::{Digest, Sha256};
    let flavor = template_id
        .split(':')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    let mut h = Sha256::new();
    let mut first = true;
    for id in sorted_ids {
        if !first {
            h.update(b",");
        }
        first = false;
        h.update(id.as_bytes());
    }
    let digest = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hash12 = String::with_capacity(12);
    for byte in &digest[..6] {
        hash12.push(HEX[(byte >> 4) as usize] as char);
        hash12.push(HEX[(byte & 0x0f) as usize] as char);
    }
    let short_ref: String = payload_ref.chars().take(7).collect();
    format!("{flavor}:{short_ref}:{hash12}")
}

/// Strip JSON `null`s a bundle may carry inside its verbatim
/// [`serde_json::Value`] fields (`landing.target`, `oracle`) so the
/// bundle can be persisted into `.aristo/canon-matches.toml` — TOML
/// cannot represent null, and toml 0.8 fails serialization on a
/// `Value::Null` inside a map/array ("unsupported unit type").
///
/// Semantically safe for the *typed* wire contract: absent keys and
/// explicit nulls decode identically (`#[serde(default)]` → `None`).
/// For the opaque `target` object this drops null-valued keys, which
/// is the graceful-degradation trade the runner makes: a stray null
/// from a future lock row must not brick every subsequent cache
/// write. Called by the runner before building a `PendingMatch`; the
/// on-the-wire bundle (types.rs golden-fixture round-trip) is NOT
/// sanitized — nulls survive JSON encode there.
pub fn sanitize_bundle_for_persistence(bundle: &mut InstrumentationBundle) {
    for record in &mut bundle.records {
        strip_json_nulls(&mut record.landing.target);
        // `target` is a required field; a degenerate all-null target
        // becomes an empty object rather than an unencodable unit.
        if record.landing.target.is_null() {
            record.landing.target = serde_json::Value::Object(serde_json::Map::new());
        }
        // Wire decode maps an explicit `oracle: null` to `None`
        // already; this guards a hand-constructed Some(Null).
        if matches!(record.oracle, Some(serde_json::Value::Null)) {
            record.oracle = None;
        }
        if let Some(oracle) = record.oracle.as_mut() {
            strip_json_nulls(oracle);
        }
    }
}

/// Recursively remove null-valued object keys and null array elements.
fn strip_json_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_json_nulls(v);
            }
        }
        serde_json::Value::Array(items) => {
            items.retain(|v| !v.is_null());
            for v in items.iter_mut() {
                strip_json_nulls(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::cache::{AcceptedMatch, CacheEntry, Disposition, PendingMatch};
    use crate::canon::types::{
        BundleCompanion, BundleCompileCheck, BundleProvenance, InstrumentationRecord, PrefixTier,
        RecordLanding, RecordPresence, VerificationMetadata,
    };
    use crate::index::AnnotationId;
    use std::collections::BTreeMap;

    fn record(accessor_id: &str) -> InstrumentationRecord {
        InstrumentationRecord {
            accessor_id: accessor_id.into(),
            kind: "inspect_projection".into(),
            class: "A".into(),
            semantic_tier: "none".into(),
            intent: format!("intent for {accessor_id}"),
            catch: format!("catch for {accessor_id}"),
            landing: RecordLanding {
                target: serde_json::json!({ "container": "LogicalLog" }),
                annotation: None,
                ensure_derive: None,
                required_use: vec![],
                companions_ref: vec![],
            },
            presence: RecordPresence {
                expected_symbol: format!("LogicalLog::{accessor_id}"),
                expected_signature: format!("fn {accessor_id}(&self) -> Option<u8>"),
                harness_probe: None,
            },
            oracle: None,
            upstream_status: "local-only".into(),
        }
    }

    fn companion(symbol: &str, file: &str) -> BundleCompanion {
        BundleCompanion {
            symbol: symbol.into(),
            role: "return_type".into(),
            file: file.into(),
            visibility: "pub (cfg aristo-instr)".into(),
            payload_ref: Some("7b6cbaec".into()),
        }
    }

    fn bundle(
        bundle_id: &str,
        payload_ref: &str,
        records: Vec<InstrumentationRecord>,
        companions: Vec<BundleCompanion>,
    ) -> InstrumentationBundle {
        let mut sut_binding = BTreeMap::new();
        sut_binding.insert("turso_core".to_string(), "core".to_string());
        InstrumentationBundle {
            bundle_id: bundle_id.into(),
            provenance: BundleProvenance {
                base_ref: "ad351877c5cf38c1fafc7f08703bfe521b8f4437".into(),
                payload_ref: payload_ref.into(),
                macro_grammar_rev: "aristo-macros 0.3.0 (two-mode Inspect grammar)".into(),
                sut_binding,
                authored_at: payload_ref.into(),
            },
            compile_check: BundleCompileCheck {
                package: "turso_core".into(),
                features: "aristo-instr,turso_core/aristo-instr".into(),
            },
            companions,
            records,
        }
    }

    const PAYLOAD: &str = "7b6cbaec04e86c0d9ac47819c77444af5054c50a";

    #[test]
    fn union_empty_input_is_empty() {
        let u = union_bundles(std::iter::empty());
        assert!(u.bundles.is_empty());
        assert!(u.warnings.is_empty());
    }

    #[test]
    fn union_dedupes_records_by_accessor_id_and_companions_by_symbol_file() {
        // Two bundles, same payload_ref, overlapping content:
        // record `alpha` and companion (W, core/types.rs) appear in
        // both and must survive exactly once; same-symbol companion
        // in a DIFFERENT file is a distinct item and must survive.
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha"), record("beta")],
            vec![companion("W", "core/types.rs")],
        );
        let b = bundle(
            "turso:7b6cbae:bbbbbbbbbbbb",
            PAYLOAD,
            vec![record("alpha"), record("gamma")],
            vec![
                companion("W", "core/types.rs"),
                companion("W", "core/wal.rs"),
            ],
        );
        let u = union_bundles([&a, &b]);
        assert!(u.warnings.is_empty(), "got: {:?}", u.warnings);
        assert_eq!(u.bundles.len(), 1);
        let merged = &u.bundles[0];
        let ids: Vec<&str> = merged
            .records
            .iter()
            .map(|r| r.accessor_id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["alpha", "beta", "gamma"],
            "first-seen input order, deduped"
        );
        let comps: Vec<(&str, &str)> = merged
            .companions
            .iter()
            .map(|c| (c.symbol.as_str(), c.file.as_str()))
            .collect();
        assert_eq!(comps, vec![("W", "core/types.rs"), ("W", "core/wal.rs")]);
        assert_eq!(merged.provenance, a.provenance);
    }

    #[test]
    fn union_identical_bundles_keeps_server_bundle_id_verbatim() {
        // The common case: every accepted suggestion carried the same
        // server-rendered bundle. The union must keep the
        // server-issued bundle_id byte-for-byte (never rederive —
        // the spec's hash-join byte is server-owned).
        let a = bundle(
            "turso:7b6cbae:ae85f8792372",
            PAYLOAD,
            vec![record("alpha"), record("beta")],
            vec![companion("W", "core/types.rs")],
        );
        let b = a.clone();
        let u = union_bundles([&a, &b]);
        assert_eq!(u.bundles.len(), 1);
        assert_eq!(u.bundles[0].bundle_id, "turso:7b6cbae:ae85f8792372");
        assert_eq!(u.bundles[0], a, "union of identical bundles is the bundle");
    }

    #[test]
    fn union_superset_input_bundle_donates_its_id() {
        // Bundle `a` covers everything `b` has; the union's record
        // set equals `a`'s, so `a`'s server-issued id is reused.
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha"), record("beta")],
            vec![],
        );
        let b = bundle(
            "turso:7b6cbae:bbbbbbbbbbbb",
            PAYLOAD,
            vec![record("beta")],
            vec![],
        );
        let u = union_bundles([&b, &a]);
        assert_eq!(u.bundles.len(), 1);
        assert_eq!(u.bundles[0].bundle_id, "turso:7b6cbae:aaaaaaaaaaaa");
    }

    #[test]
    fn union_genuine_merge_rederives_deterministic_bundle_id() {
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![],
        );
        let b = bundle(
            "turso:7b6cbae:bbbbbbbbbbbb",
            PAYLOAD,
            vec![record("beta")],
            vec![],
        );
        let u1 = union_bundles([&a, &b]);
        let u2 = union_bundles([&b, &a]);
        assert_eq!(u1.bundles.len(), 1);
        let id = &u1.bundles[0].bundle_id;
        assert_ne!(id, "turso:7b6cbae:aaaaaaaaaaaa");
        assert_ne!(id, "turso:7b6cbae:bbbbbbbbbbbb");
        // Rule shape: "<flavor>:<payload_ref[..7]>:<12 hex chars>".
        let parts: Vec<&str> = id.split(':').collect();
        assert_eq!(parts.len(), 3, "got: {id}");
        assert_eq!(parts[0], "turso");
        assert_eq!(parts[1], "7b6cbae");
        assert_eq!(parts[2].len(), 12);
        assert!(parts[2].bytes().all(|b| b.is_ascii_hexdigit()));
        // Content key: independent of input order (sorted ids).
        assert_eq!(
            u1.bundles[0].bundle_id, u2.bundles[0].bundle_id,
            "rederived id must be input-order independent"
        );
    }

    #[test]
    fn union_payload_ref_mismatch_keeps_groups_and_warns() {
        let old_payload = "1111111deadbeefdeadbeefdeadbeefdeadbeef";
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![],
        );
        let b = bundle(
            "turso:1111111:bbbbbbbbbbbb",
            old_payload,
            vec![record("alpha"), record("beta")],
            vec![],
        );
        let u = union_bundles([&a, &b]);
        assert_eq!(u.bundles.len(), 2, "one group per payload_ref");
        // Deterministic group order: sorted by payload_ref.
        assert_eq!(u.bundles[0].provenance.payload_ref, old_payload);
        assert_eq!(u.bundles[1].provenance.payload_ref, PAYLOAD);
        // Groups are NOT cross-merged: alpha appears in both.
        assert_eq!(u.bundles[0].records.len(), 2);
        assert_eq!(u.bundles[1].records.len(), 1);
        assert_eq!(u.warnings.len(), 1, "got: {:?}", u.warnings);
        assert!(
            u.warnings[0].contains("payload_ref") && u.warnings[0].contains("7b6cbaec04e"),
            "warning must name the disagreeing refs; got: {}",
            u.warnings[0]
        );
    }

    #[test]
    fn union_warns_on_intra_group_provenance_disagreement() {
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![],
        );
        let mut b = bundle(
            "turso:7b6cbae:bbbbbbbbbbbb",
            PAYLOAD,
            vec![record("beta")],
            vec![],
        );
        b.provenance.base_ref = "someotherbase".into();
        let u = union_bundles([&a, &b]);
        assert_eq!(u.bundles.len(), 1, "same payload_ref stays one group");
        assert_eq!(
            u.bundles[0].provenance.base_ref, a.provenance.base_ref,
            "first bundle's provenance block wins"
        );
        assert_eq!(u.warnings.len(), 1, "got: {:?}", u.warnings);
        assert!(u.warnings[0].contains("base_ref"), "got: {}", u.warnings[0]);
    }

    // ─── union_accepted_bundles: cache walk ───────────────────────────────

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn accepted_with(verification: Option<VerificationMetadata>) -> AcceptedMatch {
        AcceptedMatch {
            canon_id: "c".into(),
            version: "v0.1.0".into(),
            canonical_text: "t".into(),
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

    fn vm_with(bundle: InstrumentationBundle) -> VerificationMetadata {
        VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec!["wal_install_coherence".into()],
            instrumentation: Some(bundle),
        }
    }

    #[test]
    fn union_accepted_bundles_walks_accepted_matches_only() {
        let a = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![companion("W", "core/types.rs")],
        );
        let b = bundle(
            "turso:7b6cbae:bbbbbbbbbbbb",
            PAYLOAD,
            vec![record("alpha"), record("beta")],
            vec![companion("W", "core/types.rs")],
        );
        let pending_only = bundle(
            "turso:7b6cbae:cccccccccccc",
            PAYLOAD,
            vec![record("pending_only_accessor")],
            vec![],
        );

        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("aristos:one"),
            CacheEntry {
                last_match_text_hash: "blake3:x".into(),
                canon_fetched_at: "2026-07-02T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![accepted_with(Some(vm_with(a)))],
                rejected_matches: vec![],
            },
        );
        cache.entries.insert(
            aid("aristos:two"),
            CacheEntry {
                last_match_text_hash: "blake3:y".into(),
                canon_fetched_at: "2026-07-02T00:00:00Z".into(),
                pending_matches: vec![PendingMatch {
                    canon_id: "unreviewed".into(),
                    version: "v0.1.0".into(),
                    canonical_text: "t".into(),
                    canon_version: "v0.2.0".into(),
                    confidence: 0.9,
                    prefix_tier: PrefixTier::Aristos,
                    backed_by: None,
                    linked: None,
                    verification: Some(vm_with(pending_only)),
                    disposition: Disposition::Open,
                    found_at: "2026-07-02T00:00:00Z".into(),
                    found_by: "aristo stamp".into(),
                }],
                accepted_matches: vec![
                    accepted_with(Some(vm_with(b))),
                    // Pre-P-008 row and bundle-less row both skipped.
                    accepted_with(None),
                    accepted_with(Some(VerificationMetadata {
                        coverage_level: "none".into(),
                        test_binaries: vec![],
                        instrumentation: None,
                    })),
                ],
                rejected_matches: vec![],
            },
        );

        let u = union_accepted_bundles(&cache);
        assert!(u.warnings.is_empty(), "got: {:?}", u.warnings);
        assert_eq!(u.bundles.len(), 1);
        let ids: Vec<&str> = u.bundles[0]
            .records
            .iter()
            .map(|r| r.accessor_id.as_str())
            .collect();
        assert_eq!(ids, vec!["alpha", "beta"]);
        assert!(
            !ids.contains(&"pending_only_accessor"),
            "pending (unreviewed) bundles must not leak into the union"
        );
        assert_eq!(u.bundles[0].companions.len(), 1, "companions deduped");
    }

    #[test]
    fn union_accepted_bundles_empty_cache_is_empty() {
        let u = union_accepted_bundles(&CanonMatchesFile::default());
        assert!(u.bundles.is_empty());
        assert!(u.warnings.is_empty());
    }

    // ─── sanitize_bundle_for_persistence ──────────────────────────────────

    #[test]
    fn sanitize_strips_nulls_from_target_and_oracle() {
        let mut b = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![],
        );
        b.records[0].landing.target = serde_json::json!({
            "container": "LogicalLog",
            "field": "header",
            "stray": null,
            "nested": { "keep": 1, "drop": null, "arr": [1, null, 2] }
        });
        b.records[0].oracle = Some(serde_json::json!({ "assert": "x", "reachable": null }));

        sanitize_bundle_for_persistence(&mut b);

        assert_eq!(
            b.records[0].landing.target,
            serde_json::json!({
                "container": "LogicalLog",
                "field": "header",
                "nested": { "keep": 1, "arr": [1, 2] }
            })
        );
        assert_eq!(
            b.records[0].oracle,
            Some(serde_json::json!({ "assert": "x" }))
        );
        // The sanitized bundle is now TOML-encodable inside a table.
        #[derive(serde::Serialize)]
        struct Wrap {
            bundle: InstrumentationBundle,
        }
        let toml_text = toml::to_string_pretty(&Wrap { bundle: b })
            .expect("sanitized bundle must be TOML-serializable");
        assert!(!toml_text.contains("stray"), "got: {toml_text}");
    }

    #[test]
    fn sanitize_normalizes_degenerate_null_shapes() {
        let mut b = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha")],
            vec![],
        );
        b.records[0].landing.target = serde_json::Value::Null;
        b.records[0].oracle = Some(serde_json::Value::Null);
        sanitize_bundle_for_persistence(&mut b);
        assert_eq!(
            b.records[0].landing.target,
            serde_json::json!({}),
            "all-null target becomes an empty object, not an unencodable unit"
        );
        assert_eq!(b.records[0].oracle, None);
    }

    #[test]
    fn sanitize_is_a_noop_on_a_null_free_bundle() {
        let mut b = bundle(
            "turso:7b6cbae:aaaaaaaaaaaa",
            PAYLOAD,
            vec![record("alpha"), record("beta")],
            vec![companion("W", "core/types.rs")],
        );
        let before = b.clone();
        sanitize_bundle_for_persistence(&mut b);
        assert_eq!(b, before);
    }
}
