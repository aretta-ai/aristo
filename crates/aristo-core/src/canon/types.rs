//! Wire types for the canon API contract.
//!
//! Mirrors `../aretta-sdk/docs/mockups/13-canon-and-matching/README.md` §L3
//! byte-for-byte for `POST /canon/match`, `GET /canon/entry/<id>`,
//! `POST /canon/request-verify`. Slice 3 (aretta-code) consumes the
//! same shapes; cross-slice drift here breaks both ends.
//!
//! All types serialize to JSON (the wire format) and can also be
//! deserialized from TOML (for mock-client fixtures and the
//! `.aristo/canon-matches.toml` cache). Wire JSON uses `snake_case`
//! field names (per the README L3 examples: `canon_id`,
//! `canonical_text`, `prefix_tier`, `effective_scopes`); serde's
//! default field-name behavior is the correct match.
//!
//! ## What's user-visible vs. server-side-only
//!
//! The full canon entry has a `verification_artifacts` field
//! containing formal models / neural prompts / test corpora. Per
//! canon-strategy.md §CS10 ("what the card deliberately hides"),
//! that field is server-side only and **never** reaches the user.
//! [`CanonEntry`] below does not have a field for it.
//!
//! ## Phase 1 scope (per `_deferred/verification-execution.md`)
//!
//! - The `verification` block in [`CanonMatch`] is **informational**
//!   metadata about what Phase 2 will eventually run. It is not an
//!   execution trigger in Phase 1. Field stays in the contract
//!   because Slice 3 surfaces it from coverage.yaml; the SDK ignores
//!   it until Phase 2.
//! - There is no `verified_outcome` field anywhere in the wire
//!   types — that surface lands in Phase 2 alongside the
//!   verification execution endpoint.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ─── POST /canon/match ─────────────────────────────────────────────────────

/// Request body for `POST /canon/match`. Batched: a single call
/// covers all annotations the SDK wants matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonMatchRequest {
    /// One entry per annotation to match. Server returns
    /// [`CanonMatchResponse::results`] aligned to this list by
    /// index. Empty list is a valid request that returns an empty
    /// `results` list.
    pub annotations: Vec<AnnotationMatchInput>,
    /// Client-sent confidence floor. Honored above the server's
    /// enforced floor of `0.5` (below that, server responds HTTP 400).
    /// `stamp` sends `0.85`; `critique` sends `0.65`; tunable via
    /// `aristo.toml [canon] threshold_*`.
    pub confidence_threshold: f64,
    /// Opt-in to the §17 proof-tree suggestions channel. When `true`,
    /// the server expands each primary match into its proof-objective
    /// cluster and attaches [`CanonMatchResponse::suggestions`].
    /// Defaults to `false` so the existing match path stays
    /// byte-identical on the wire (back-compat: old clients never send
    /// the field; the server treats absence as `false`).
    #[serde(default)]
    pub include_suggestions: bool,
}

/// One annotation-shaped input to the batched match call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationMatchInput {
    /// The annotation's text-to-match. Sent verbatim — server-side
    /// matching against `match_signals.required_terms` /
    /// `synonym_sets` happens against this string.
    pub annotation_text: String,
    /// The annotation's `applies_to` set (`fn`, `method`, `mod`,
    /// `struct`, `trait`). Used by the server to filter candidate
    /// entries (a fn-only canon entry shouldn't match a struct-level
    /// annotation).
    pub applies_to: Vec<String>,
}

/// Response body for `POST /canon/match`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonMatchResponse {
    /// Aligned to [`CanonMatchRequest::annotations`] by index.
    /// Each inner list is the top-N candidates for that annotation
    /// (top-3 if `(max - 3rd_max) < 0.10`, else top-1, per the
    /// multi-match policy in canon-strategy.md / README.md L3).
    pub results: Vec<Vec<CanonMatch>>,
    /// The set of canon scopes that produced matches for the
    /// requesting repo. Always contains `":vanilla"`; DP/Enterprise
    /// repos additionally see their named flavors here.
    pub effective_scopes: Vec<String>,
    /// Catalog-level snapshot tag from canon-strategy.md §CS12.
    /// Informational; per-entry version (`CanonMatch::version`) is
    /// the load-bearing cache key.
    pub canon_version: String,
    /// ISO-8601 timestamp of when the server computed this response.
    pub matched_at: String,
    /// §17 proof-tree suggestions, aligned by annotation index to
    /// [`results`](Self::results). One [`ClusterSuggestion`] per
    /// requested annotation; entries are `None` where the annotation
    /// had no cluster. The whole field is `None`/absent for clients
    /// that did not set [`CanonMatchRequest::include_suggestions`]
    /// (back-compat — old responses decode with `suggestions = None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<Option<ClusterSuggestion>>>,
}

// ─── §17 proof-tree suggestions ────────────────────────────────────────────

/// One proof-objective cluster pulled in for a primary match. Mirrors
/// `ClusterSuggestion` in `contract/suggestions.schema.json`.
///
/// The cluster hangs off the primary `for_canon_id`; it carries the
/// `objective` (the `kanon:` proof-objective parent — `None` until
/// Slice 0b authors objective entries) and the `siblings` (co-member
/// leaf entries, deduped, with the primary excluded by the server —
/// "dedup ①").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClusterSuggestion {
    /// The primary matched `canon_id` this cluster hangs off of.
    pub for_canon_id: String,
    /// The `kanon:` proof-objective (parent). `None` until Slice 0b
    /// authors objective entries (siblings-only mode).
    pub objective: Option<SuggestedEntry>,
    /// Co-member leaf entries, deduped, with the primary excluded
    /// (server "dedup ①"). May be empty.
    pub siblings: Vec<SuggestedEntry>,
}

/// One suggested canon entry within a [`ClusterSuggestion`] — either
/// the objective (parent) or a sibling leaf. Mirrors `SuggestedEntry`
/// in `contract/suggestions.schema.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedEntry {
    pub canon_id: String,
    pub version: String,
    pub canonical_text: String,
    pub scope: String,
    pub prefix_tier: PrefixTier,
    /// `Some(_)` for `aristos:` tier; `None` for `kanon:` tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backed_by: Option<String>,
    /// Scope-aware verification metadata. Informational in Phase 1
    /// (same carve-out as [`CanonMatch::verification`]). Defaults to
    /// the empty metadata when the server omits it.
    #[serde(default)]
    pub verification: VerificationMetadata,
    /// Where this entry sits relative to the primary. v1 emits only
    /// `parent` (the objective) + `sibling` (co-member leaves).
    pub relationship: Relationship,
}

/// Where a [`SuggestedEntry`] sits relative to the primary match.
/// Mirrors the `relationship` enum in
/// `contract/suggestions.schema.json`. v1 emits only `Parent` +
/// `Sibling`; `Child` is reserved for the future Lean-DAG layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Relationship {
    /// The proof-objective node — the cluster's parent.
    Parent,
    /// A co-member leaf obligation under the same objective.
    Sibling,
    /// Reserved for the future Lean-DAG sub-obligation layer.
    Child,
}

/// One match candidate for one annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonMatch {
    /// Canonical id (the readable suffix, without prefix). The
    /// `prefix_tier` field determines which prefix the SDK applies
    /// on accept (`aristos:<canon_id>` or `kanon:<canon_id>`).
    pub canon_id: String,
    /// Per-entry version pinned at match time per canon-strategy.md
    /// §CS12. Cache key for `.aristo/canon-matches.toml`.
    pub version: String,
    /// The canonical phrasing. On accept, the SDK rewrites the
    /// annotation's source text to this string.
    pub canonical_text: String,
    /// Match score in `[0.0, 1.0]`. Above the request's
    /// `confidence_threshold` by construction; the server filters
    /// candidates server-side.
    pub confidence: f64,
    /// Which scope the candidate came from. Always one of
    /// [`CanonMatchResponse::effective_scopes`]; usually
    /// `":vanilla"`, sometimes a named flavor for DP/Enterprise.
    pub scope: String,
    /// Which prefix tier this match graduates to on accept.
    pub prefix_tier: PrefixTier,
    /// The canon entry's declared verification mechanism for the
    /// user's scope. `Some(_)` when `prefix_tier ==
    /// PrefixTier::Aristos`; `None` when `prefix_tier ==
    /// PrefixTier::Kanon`. The freeform string vocabulary is
    /// documented in canon-strategy.md §CS10 ("Backed by" — initial
    /// freeform vocabulary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backed_by: Option<String>,
    /// Opaque server-issued binding handle. Per canon-strategy.md
    /// §CS10 + `DECISIONS.md` §B5b, this is a 128-bit-random
    /// `arta_<base32>` ref that uniquely identifies *this binding
    /// event* server-side — distinct from `canon_id` (which is shared
    /// across every user that binds to the same catalog entry) and
    /// from the source-level `annotation_id` (which is user-chosen
    /// and unknown to the server). Phase 2 includes `linked` inside
    /// the signed `verified_outcome` tuple, so a signed outcome
    /// cannot be lifted between bindings.
    ///
    /// **Phase 1 carve-out.** Optional on the wire because the current
    /// dev/prod proxy doesn't emit it yet (see the
    /// `match_response_decodes_when_server_omits_linked` test for the
    /// production fixture). When the server omits it, the SDK
    /// synthesizes a deterministic placeholder at accept time in
    /// `canon/accept.rs` so the `BindingState::Bound { linked }`
    /// invariant downstream still holds. The trust card hides this
    /// field either way per CS10's "what the card deliberately hides".
    ///
    /// **Phase 2 must restore the required contract.** See
    /// `../docs/mockups/13-canon-and-matching/_deferred/verification-execution.md`
    /// — when `verified_outcome` lands, the signed-tuple integrity
    /// check needs a server-issued (high-entropy) `linked`, not a
    /// client-derived placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<String>,
    /// Scope-aware verification metadata from `coverage.yaml`.
    /// **Informational only in Phase 1** — Phase 2's verification
    /// execution endpoint reads this to route the test binary, but
    /// the Phase 1 SDK ignores it. See
    /// `_deferred/verification-execution.md`.
    pub verification: VerificationMetadata,
}

/// Which prefix tier a canon match graduates to on accept. Per
/// canon-strategy.md §CS13.
///
/// Wire-form is the prefix string with the colon
/// (`"aristos:"` / `"kanon:"`) — matching the source-code form and
/// the sample-matches.toml `prefix_tier` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PrefixTier {
    /// Backed — canon entry has populated `backed_by` for the user's
    /// scope. Source id gains `aristos:` prefix on accept.
    #[serde(rename = "aristos:")]
    Aristos,
    /// Unbacked — canon entry has no `backed_by` for the user's
    /// scope yet. Source id gains `kanon:` prefix on accept.
    #[serde(rename = "kanon:")]
    Kanon,
}

impl PrefixTier {
    /// Returns the source-form prefix string with trailing colon.
    pub fn as_prefix(self) -> &'static str {
        match self {
            PrefixTier::Aristos => "aristos:",
            PrefixTier::Kanon => "kanon:",
        }
    }
}

/// Synthesize a Phase 1 placeholder [`crate::index::ArtaId`] from `(canon_id, version)`
/// when the canon server omits the `linked` field from `/canon/match`.
///
/// **Phase 1 carve-out.** Per the `linked` rationale on
/// [`CanonMatch::linked`], the server-issued opaque ref is meant to be
/// a 128-bit-random handle that uniquely identifies this binding event.
/// The current dev/prod proxy doesn't emit it yet, but the SDK still
/// needs *some* `ArtaId` to fill the index entry's
/// `BindingState::Bound { linked }` slot. We synthesize a deterministic
/// placeholder from `sha256(canon_id, version)` — Phase 1 never reads
/// `linked`, so the value is informational; Phase 2's verified_outcome
/// signing pipeline requires the server-issued value and will reject
/// any placeholder it can identify as client-derived.
///
/// **Determinism.** Two callers binding the same `(canon_id, version)`
/// pair get the same synthesized id — useful for idempotency, but
/// **not** unique per binding instance. That's the property Phase 2's
/// server-issued `linked` provides; synthesized placeholders don't.
///
/// **Migration plan.** When the server starts emitting `linked`, the
/// SDK keeps reading it from the wire (the `Option<String>` field).
/// Existing cache + index entries created with synthesized placeholders
/// stay valid for `BindingState::Bound` — they'll only be invalidated
/// if/when the user runs a future `aristo canon refresh` that surfaces
/// the real server-issued id, OR when Phase 2's verified_outcome lands
/// and rebinding becomes a separate user-driven step.
pub fn synthesize_phase1_linked(canon_id: &str, version: &str) -> crate::index::ArtaId {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"aristo-phase1-linked-synth\0");
    h.update(canon_id.as_bytes());
    h.update(b"\0");
    h.update(version.as_bytes());
    let digest = h.finalize();
    // ArtaId schema is `arta_<8-64 alphanumeric>`. Use the first 16
    // bytes of the digest as 32 lowercase-hex chars → 128 bits of
    // derived entropy fits the format.
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut body = String::with_capacity(32);
    for byte in &digest[..16] {
        body.push(HEX[(byte >> 4) as usize] as char);
        body.push(HEX[(byte & 0x0f) as usize] as char);
    }
    crate::index::ArtaId::parse(&format!("arta_{body}"))
        .expect("synthesized arta_ id must satisfy ArtaId schema")
}

/// Scope-aware verification metadata surfaced by the match response.
/// Mirrors the coverage.yaml routing manifest's per-entry fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationMetadata {
    /// `"tight"` | `"partial"` | `"informal"` | `"none"` per
    /// canon-strategy.md (the `relation` field on coverage entries).
    pub coverage_level: String,
    /// Names of test binaries the server would run for this match
    /// in Phase 2. Phase 1 ignores this list.
    pub test_binaries: Vec<String>,
}

// ─── GET /canon/entry/<id>?version=<v> ─────────────────────────────────────

/// Full per-entry detail surfaced by `GET /canon/entry/<canon_id>`.
/// Used to render the bound-annotation trust card + the
/// `aristo canon show <id>` drill-down.
///
/// Shape pinned by §13 README L2 — "**`backed_by` map per scope**"
/// is the locked spec, not flat. Closed-IP fields are deliberately
/// absent (canon-strategy.md §CS10): `alternative_phrasings`,
/// `verification_artifacts`, internal spec IDs, the opaque server
/// `linked` ref.
///
/// The server-side projection (toolsaurus's `buildEntryCard`) is
/// scope-aware: `backed_by` + `prefix_tier_by_scope` are filtered to
/// the caller's effective scopes, and `references.related_entries`
/// is filtered to canon_ids known to be active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonEntry {
    /// Canonical id (readable, no prefix).
    pub canon_id: String,
    /// Per-entry version requested (`v<minor>.<patch>`). May differ
    /// from [`active_version`](Self::active_version) when the caller
    /// passes `?version=<v>` to inspect a deprecated entry.
    pub version: String,
    /// The current active version for this `canon_id` per the
    /// canon-strategy.md §CS12 immutability model. When
    /// `version != active_version`, this entry is the deprecated
    /// version surfaced via the explicit `?version=` query.
    pub active_version: String,
    /// True when [`version`](Self::version) is not the active
    /// version. Trust card UI should signal "deprecated; pinning to
    /// active recommended."
    pub is_deprecated: bool,
    /// Catalog snapshot tag from canon-strategy.md §CS12.
    /// Informational; matches the `canon_version` in the parent
    /// match response that surfaced this entry.
    pub canon_version: String,
    /// The natural-language statement. Source-of-truth field for
    /// the trust card's statement paragraph.
    pub canonical_text: String,
    /// Where the entry applies (`fn`, `method`, `mod`, `struct`,
    /// `trait`).
    pub applies_to: Vec<String>,
    /// `concurrency` | `validation` | `lifecycle` | `error-handling`
    /// | `invariants` | `resources` | `other`.
    pub category: String,
    /// `safety` | `liveness` | `functional-correctness` |
    /// `termination`.
    pub property_type: String,
    /// Declared verification mechanism, **per effective scope** (per
    /// README §L2). Keyed by scope string (`":vanilla"`, `"turso"`,
    /// etc.); value is `Some(_)` when the entry has a backing for
    /// that scope (`aristos:` tier — see
    /// [`prefix_tier_by_scope`](Self::prefix_tier_by_scope)) and
    /// `None` for an unbacked scope (`kanon:` tier).
    ///
    /// Filtered server-side to the caller's effective scopes — keys
    /// outside the caller's scope set never appear.
    #[serde(default)]
    pub backed_by: BTreeMap<String, Option<String>>,
    /// Prefix tier for each scope present in
    /// [`backed_by`](Self::backed_by). `kanon:` when the scope's
    /// `backed_by` value is `None`; `aristos:` when populated.
    /// Mirrors what the SDK applies as the source-level id prefix on
    /// `aristo canon accept`.
    #[serde(default)]
    pub prefix_tier_by_scope: BTreeMap<String, PrefixTier>,
    /// Longer pedagogical description, free-form prose. Empty string
    /// when the catalog entry doesn't set one.
    #[serde(default)]
    pub description: String,
    /// Abstract example shapes. The trust card surfaces
    /// `examples[0]` labeled "abstract — not your code."
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// Informal pseudo-code / mathematical sketch of the invariant.
    /// Empty string when the catalog entry doesn't set one.
    #[serde(default)]
    pub invariant_sketch: String,
    /// Cross-references for the trust card's References block.
    #[serde(default)]
    pub references: References,
    /// The set of scopes the server used to compute this card. The
    /// keys of [`backed_by`](Self::backed_by) and
    /// [`prefix_tier_by_scope`](Self::prefix_tier_by_scope) are a
    /// subset of this set.
    #[serde(default)]
    pub effective_scopes: Vec<String>,
}

/// Cross-references surfaced on the trust card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct References {
    /// Citations (e.g., academic papers, books).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub literature: Vec<String>,
    /// Related canon entries **filtered to the user's effective
    /// scopes** server-side. Bare canon_id strings — to render with
    /// prefix tier, the renderer needs to call
    /// [`get_entry`](crate::canon::client::CanonClient::get_entry)
    /// per id (or surface them as bare ids). Phase 1 renders bare.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_entries: Vec<String>,
    /// External URLs (e.g., blog posts, design docs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external: Vec<String>,
}

// ─── POST /canon/request-verify ───────────────────────────────────────────

/// Request body for `POST /canon/request-verify`. Idempotent on
/// `(canon_id, repo_full_name, user_id)` per canon-strategy.md
/// §CS11.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequestVerifyBody {
    /// The canon entry the user is signaling demand for.
    pub canon_id: String,
    /// Optional free-text context from the user (e.g., "critical
    /// for our financial-tx audit"). On repeat calls with a new
    /// note, replaces the previous note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Response for `POST /canon/request-verify`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequestVerifyResponse {
    /// `"submitted"` for first-time requests; `"updated"` for
    /// repeat requests (idempotency surface).
    pub status: String,
    pub canon_id: String,
    /// The canon entry's current `backed_by` value, if any. Mirrors
    /// what the user sees on the trust card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_backing: Option<String>,
    /// ISO-8601 timestamp of the prior submission, when `status ==
    /// "updated"`. `None` for first-time requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previously_submitted_at: Option<String>,
}

// ─── GET /catalogue ─────────────────────────────────────────────────────────

/// Response for `GET /catalogue` — the full active canon corpus
/// catalogue (one entry per canon id, at its active version). Closed-IP
/// fields (alternative phrasings, match signals) are stripped
/// server-side; this is the browsable trust-card surface. Served by a
/// per-repo conductor; addressed via the resolved data-plane base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonCatalogue {
    /// Server-stamped proprietary / confidential notice (one line per
    /// element), written at the top of the downloaded snapshot so the
    /// marking travels with this proprietary corpus. Empty from servers
    /// that don't send it (older conductors).
    #[serde(default)]
    pub notice: Vec<String>,
    #[serde(default)]
    pub entries: Vec<CanonCatalogueEntry>,
}

/// One catalogue entry: the active version of a canon id (the server
/// sorts entries by `(category, canon_id)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanonCatalogueEntry {
    pub canon_id: String,
    pub version: String,
    pub canonical_text: String,
    pub category: String,
    #[serde(default)]
    pub applies_to: Vec<String>,
    /// Scopes that back this canon (scope -> backing string). Empty =
    /// unbacked (the `kanon:` tier).
    #[serde(default)]
    pub backed_by: BTreeMap<String, String>,
    pub coverage_level: String,
    /// Spec ids (`S-XXX`) this canon references — the join key to the
    /// coverage map.
    #[serde(default)]
    pub spec_refs: Vec<String>,
}

impl CanonCatalogueEntry {
    /// Tier label derived from backing, mirroring the dashboard: an
    /// entry backed by any scope is `aristos`, otherwise `kanon`. The
    /// wire carries `backed_by`, not a tier field; this is the sole
    /// derivation point so the CLI and any other consumer agree.
    pub fn tier_label(&self) -> &'static str {
        if self.backed_by.is_empty() {
            "kanon"
        } else {
            "aristos"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_tier_serializes_with_colon() {
        // Source-form prefix string — matches `prefix_tier` field in
        // sample-matches.toml verbatim.
        let v = serde_json::to_value(PrefixTier::Aristos).unwrap();
        assert_eq!(v, serde_json::json!("aristos:"));
        let v = serde_json::to_value(PrefixTier::Kanon).unwrap();
        assert_eq!(v, serde_json::json!("kanon:"));
    }

    #[test]
    fn prefix_tier_deserializes_with_colon() {
        let v: PrefixTier = serde_json::from_str("\"aristos:\"").unwrap();
        assert_eq!(v, PrefixTier::Aristos);
        let v: PrefixTier = serde_json::from_str("\"kanon:\"").unwrap();
        assert_eq!(v, PrefixTier::Kanon);
    }

    #[test]
    fn prefix_tier_rejects_unknown() {
        assert!(serde_json::from_str::<PrefixTier>("\"aristos\"").is_err());
        assert!(serde_json::from_str::<PrefixTier>("\"backed\"").is_err());
        assert!(serde_json::from_str::<PrefixTier>("\"\"").is_err());
    }

    #[test]
    fn prefix_tier_as_prefix_matches_serialized_form() {
        assert_eq!(PrefixTier::Aristos.as_prefix(), "aristos:");
        assert_eq!(PrefixTier::Kanon.as_prefix(), "kanon:");
    }

    #[test]
    fn match_request_round_trips_via_json() {
        let req = CanonMatchRequest {
            annotations: vec![AnnotationMatchInput {
                annotation_text: "each cell should be written exactly once per page edit".into(),
                applies_to: vec!["fn".into(), "method".into()],
            }],
            confidence_threshold: 0.85,
            include_suggestions: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CanonMatchRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn match_response_aristos_tier_round_trips() {
        let resp = CanonMatchResponse {
            results: vec![vec![CanonMatch {
                canon_id: "cell_written_exactly_once_per_page_edit".into(),
                version: "v0.2.1".into(),
                canonical_text: "edit_page writes each cell exactly once".into(),
                confidence: 0.92,
                scope: ":vanilla".into(),
                prefix_tier: PrefixTier::Aristos,
                backed_by: Some("specialized neural checker".into()),
                linked: Some("arta_a1b2c3d4".into()),
                verification: VerificationMetadata {
                    coverage_level: "tight".into(),
                    test_binaries: vec!["monotonicity_property".into()],
                },
            }]],
            effective_scopes: vec![":vanilla".into()],
            canon_version: "v0.2.0".into(),
            matched_at: "2026-06-15T09:14:22Z".into(),
            suggestions: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CanonMatchResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn match_response_kanon_tier_omits_backed_by() {
        // When prefix_tier == Kanon, backed_by is None; serialized
        // form should omit the field per skip_serializing_if.
        let m = CanonMatch {
            canon_id: "checkout_total_non_negative".into(),
            version: "v0.1.0".into(),
            canonical_text: "checkout total is non-negative".into(),
            confidence: 0.94,
            scope: ":vanilla".into(),
            prefix_tier: PrefixTier::Kanon,
            backed_by: None,
            linked: Some("arta_b2c3d4e5".into()),
            verification: VerificationMetadata {
                coverage_level: "none".into(),
                test_binaries: vec![],
            },
        };
        let json = serde_json::to_string(&m).unwrap();
        // backed_by should NOT appear in the JSON.
        assert!(
            !json.contains("backed_by"),
            "expected backed_by to be omitted, got: {json}"
        );
        // Round-trip still works.
        let back: CanonMatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.prefix_tier, PrefixTier::Kanon);
    }

    #[test]
    fn match_response_decodes_when_server_omits_linked() {
        // Phase 1 carve-out — see canon-strategy.md §CS10 plus
        // ../docs/mockups/13-canon-and-matching/_deferred/verification-execution.md
        // for the Phase 2 plan that restores `linked` to required.
        //
        // Production fixture: copy of the JSON dev.aretta.ai actually
        // returns for `POST /canon/match` today (no `linked` in any
        // per-match record). Verified by curl on 2026-05-22 during
        // the live-dev runbook's first run.
        let raw = r#"{
            "results": [
                [
                    {
                        "canon_id": "waste_no_time_arguing_be_a_good_man",
                        "version": "v0.1.0",
                        "canonical_text": "Waste no more time arguing what a good man should be. Be one.",
                        "confidence": 0.9999999999999998,
                        "scope": ":vanilla",
                        "prefix_tier": "kanon:",
                        "backed_by": null,
                        "verification": {
                            "coverage_level": "none",
                            "test_binaries": []
                        }
                    }
                ]
            ],
            "effective_scopes": [":vanilla"],
            "canon_version": "v0.1.0",
            "matched_at": "2026-05-22T18:04:51.080Z"
        }"#;
        let resp: CanonMatchResponse =
            serde_json::from_str(raw).expect("dev's real /canon/match response should decode");
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].len(), 1);
        let m = &resp.results[0][0];
        assert_eq!(m.canon_id, "waste_no_time_arguing_be_a_good_man");
        assert_eq!(m.prefix_tier, PrefixTier::Kanon);
        // The Phase 1 contract: `linked` is optional, server may omit
        // it. When omitted, decode succeeds with `linked = None`; the
        // SDK synthesizes a deterministic placeholder at accept time.
        assert!(
            m.linked.is_none(),
            "expected linked to be None when server omits it, got: {:?}",
            m.linked
        );
    }

    #[test]
    fn synthesize_phase1_linked_is_deterministic_and_well_formed() {
        let a = synthesize_phase1_linked("waste_no_time_arguing_be_a_good_man", "v0.1.0");
        let b = synthesize_phase1_linked("waste_no_time_arguing_be_a_good_man", "v0.1.0");
        let c = synthesize_phase1_linked("waste_no_time_arguing_be_a_good_man", "v0.1.1");
        let d = synthesize_phase1_linked("obstacle_is_the_way", "v0.1.0");
        assert_eq!(a, b, "same (canon_id, version) → same arta_ id");
        assert_ne!(a, c, "version difference must change the synthesized id");
        assert_ne!(a, d, "canon_id difference must change the synthesized id");
        // Schema check: 32 lowercase hex chars after the `arta_` prefix.
        let s = a.as_str();
        assert!(s.starts_with("arta_"), "must start with arta_");
        assert_eq!(s.len(), 5 + 32, "body should be 32 hex chars");
        assert!(s[5..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn match_response_omits_linked_field_when_none() {
        // Serialization side: if the SDK ever holds a CanonMatch with
        // linked = None (e.g. round-tripping a decoded dev response),
        // re-serializing should not emit the field at all — keeps the
        // wire shape clean and compatible with the proxy's redaction
        // logic for the /canon/entry sibling endpoint.
        let m = CanonMatch {
            canon_id: "checkout_total_non_negative".into(),
            version: "v0.1.0".into(),
            canonical_text: "checkout total is non-negative".into(),
            confidence: 0.94,
            scope: ":vanilla".into(),
            prefix_tier: PrefixTier::Kanon,
            backed_by: None,
            linked: None,
            verification: VerificationMetadata {
                coverage_level: "none".into(),
                test_binaries: vec![],
            },
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("\"linked\""),
            "expected linked to be omitted when None, got: {json}"
        );
    }

    #[test]
    fn canon_entry_round_trips_with_per_scope_backed_by() {
        let mut backed_by = BTreeMap::new();
        backed_by.insert(
            ":vanilla".to_string(),
            Some("specialized neural checker".to_string()),
        );
        backed_by.insert(
            "turso".to_string(),
            Some("golden model + differential testing".to_string()),
        );
        let mut prefix_tier_by_scope = BTreeMap::new();
        prefix_tier_by_scope.insert(":vanilla".to_string(), PrefixTier::Aristos);
        prefix_tier_by_scope.insert("turso".to_string(), PrefixTier::Aristos);
        let entry = CanonEntry {
            canon_id: "cell_written_exactly_once_per_page_edit".into(),
            version: "v0.2.1".into(),
            active_version: "v0.2.1".into(),
            is_deprecated: false,
            canon_version: "v0.2.0".into(),
            canonical_text: "a page-edit operation writes each cell exactly once".into(),
            applies_to: vec!["fn".into(), "method".into()],
            category: "invariants".into(),
            property_type: "safety".into(),
            backed_by,
            prefix_tier_by_scope,
            description: "Standard concurrency invariant".into(),
            examples: vec!["fn edit(...) { ... }".into()],
            invariant_sketch: "forall e, |writes(e)| == |cells(page)|".into(),
            references: References {
                literature: vec!["Lamport (CACM 20:11, 1977)".into()],
                related_entries: vec![
                    "balance_no_duplicate_cells".into(),
                    "edit_atomicity_per_page".into(),
                ],
                external: vec![],
            },
            effective_scopes: vec![":vanilla".into(), "turso".into()],
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: CanonEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn canon_entry_decodes_live_dev_fixture() {
        // Production fixture: copy of what dev.aretta.ai returns for
        // GET /canon/entry/waste_no_time_arguing_be_a_good_man on
        // 2026-05-22, post the entry-endpoint migration to
        // toolsaurus. Pins the spec-locked "backed_by map per scope"
        // shape from README §L2.
        let raw = r#"{
            "canon_id": "waste_no_time_arguing_be_a_good_man",
            "version": "v0.1.0",
            "active_version": "v0.1.0",
            "is_deprecated": false,
            "canon_version": "v0.1.0",
            "canonical_text": "Waste no more time arguing what a good man should be. Be one.",
            "applies_to": ["fn", "method"],
            "category": "other",
            "property_type": "safety",
            "backed_by": { ":vanilla": null },
            "prefix_tier_by_scope": { ":vanilla": "kanon:" },
            "description": "Aurelian easter-egg entry exercising the kanon: tier under :vanilla scope.",
            "examples": [],
            "invariant_sketch": "",
            "references": {
                "literature": ["Aurelius, Marcus. Meditations, Book X.16."],
                "related_entries": ["obstacle_is_the_way"],
                "external": []
            },
            "effective_scopes": [":vanilla"]
        }"#;
        let entry: CanonEntry = serde_json::from_str(raw).expect("live-dev fixture must decode");
        assert_eq!(entry.canon_id, "waste_no_time_arguing_be_a_good_man");
        assert_eq!(entry.backed_by.get(":vanilla"), Some(&None));
        assert_eq!(
            entry.prefix_tier_by_scope.get(":vanilla"),
            Some(&PrefixTier::Kanon)
        );
        assert_eq!(
            entry.references.related_entries,
            vec!["obstacle_is_the_way"]
        );
    }

    #[test]
    fn request_verify_body_omits_optional_notes() {
        let body = RequestVerifyBody {
            canon_id: "foo".into(),
            notes: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("notes"), "expected notes to be omitted");
        let back: RequestVerifyBody = serde_json::from_str(&json).unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn request_verify_response_first_time_omits_previous_timestamp() {
        let resp = RequestVerifyResponse {
            status: "submitted".into(),
            canon_id: "foo".into(),
            current_backing: Some("specialized neural checker".into()),
            previously_submitted_at: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("previously_submitted_at"));
        let back: RequestVerifyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn request_verify_response_repeat_includes_timestamp() {
        let resp = RequestVerifyResponse {
            status: "updated".into(),
            canon_id: "foo".into(),
            current_backing: Some("specialized neural checker".into()),
            previously_submitted_at: Some("2026-06-15T09:14:22Z".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("previously_submitted_at"));
        let back: RequestVerifyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn relationship_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(Relationship::Parent).unwrap(),
            serde_json::json!("parent")
        );
        assert_eq!(
            serde_json::to_value(Relationship::Sibling).unwrap(),
            serde_json::json!("sibling")
        );
        assert_eq!(
            serde_json::to_value(Relationship::Child).unwrap(),
            serde_json::json!("child")
        );
        let back: Relationship = serde_json::from_str("\"parent\"").unwrap();
        assert_eq!(back, Relationship::Parent);
    }

    #[test]
    fn match_response_without_suggestions_omits_field() {
        // include_suggestions defaults to false; the response carries
        // no suggestions key — byte-identical to the pre-§17 wire shape.
        let resp = CanonMatchResponse {
            results: vec![],
            effective_scopes: vec![":vanilla".into()],
            canon_version: "v0.2.0".into(),
            matched_at: "2026-06-15T09:14:22Z".into(),
            suggestions: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(
            !json.contains("suggestions"),
            "expected suggestions to be omitted, got: {json}"
        );
    }

    #[test]
    fn old_response_decodes_with_suggestions_none() {
        // Back-compat: a pre-§17 server response has no `suggestions`
        // key. `#[serde(default)]` must decode it as `None`.
        let raw = r#"{
            "results": [],
            "effective_scopes": [":vanilla"],
            "canon_version": "v0.1.0",
            "matched_at": "2026-05-22T18:04:51.080Z"
        }"#;
        let resp: CanonMatchResponse = serde_json::from_str(raw).unwrap();
        assert!(resp.suggestions.is_none());
    }

    #[test]
    fn match_request_serializes_with_snake_case_keys() {
        // Important: server and SDK both expect snake_case wire form.
        // Field rename to camelCase here would break Slice 3.
        let req = CanonMatchRequest {
            annotations: vec![AnnotationMatchInput {
                annotation_text: "x".into(),
                applies_to: vec!["fn".into()],
            }],
            confidence_threshold: 0.85,
            include_suggestions: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("annotation_text"), "got: {json}");
        assert!(json.contains("applies_to"), "got: {json}");
        assert!(json.contains("confidence_threshold"), "got: {json}");
    }

    #[test]
    fn catalogue_tier_label_derives_from_backing() {
        let mut backed = BTreeMap::new();
        backed.insert(":vanilla".to_string(), "neural checker".to_string());
        let backed_entry = CanonCatalogueEntry {
            canon_id: "a".into(),
            version: "v0.1.0".into(),
            canonical_text: "a".into(),
            category: "invariants".into(),
            applies_to: vec!["fn".into()],
            backed_by: backed,
            coverage_level: "tight".into(),
            spec_refs: vec!["S-001".into()],
        };
        let unbacked_entry = CanonCatalogueEntry {
            backed_by: BTreeMap::new(),
            ..backed_entry.clone()
        };
        assert_eq!(backed_entry.tier_label(), "aristos");
        assert_eq!(unbacked_entry.tier_label(), "kanon");
    }

    #[test]
    fn catalogue_json_round_trips_and_tolerates_empty() {
        let cat = CanonCatalogue {
            notice: vec!["PROPRIETARY & CONFIDENTIAL".to_string()],
            entries: vec![CanonCatalogueEntry {
                canon_id: "a".into(),
                version: "v0.1.0".into(),
                canonical_text: "text".into(),
                category: "invariants".into(),
                applies_to: vec!["fn".into()],
                backed_by: BTreeMap::new(),
                coverage_level: "none".into(),
                spec_refs: vec![],
            }],
        };
        let json = serde_json::to_string(&cat).unwrap();
        let back: CanonCatalogue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cat);
        // The notice serializes FIRST (top of the downloaded snapshot).
        assert!(
            json.find("notice").unwrap() < json.find("entries").unwrap(),
            "notice must serialize before entries: {json}"
        );
        // The server sends {"entries":[]} when no canon dir is configured,
        // and the notice defaults to empty for older servers that omit it.
        let empty: CanonCatalogue = serde_json::from_str(r#"{"entries":[]}"#).unwrap();
        assert!(empty.entries.is_empty());
        assert!(empty.notice.is_empty());
    }
}
