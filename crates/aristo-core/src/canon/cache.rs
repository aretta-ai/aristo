//! `.aristo/canon-matches.toml` schema + atomic I/O.
//!
//! The on-disk shape mirrors the worked example at
//! `../aretta-sdk/docs/mockups/13-canon-and-matching/sample-matches.toml`:
//!
//! ```toml
//! [__meta__]
//! schema_version = 1
//! canon_version  = "v0.2.0"
//! last_fetched   = "2026-06-15T09:14:22Z"
//!
//! [my_annotation_id]
//! last_match_text_hash = "blake3:..."
//! canon_fetched_at     = "2026-06-15T09:14:22Z"
//!
//! [[my_annotation_id.pending_matches]]
//! canon_id       = "..."
//! ...
//! ```
//!
//! Top-level shape: `[__meta__]` + one top-level table per annotation id.
//! Canon-prefixed ids (e.g., `aristos:foo`, `kanon:bar`) use the
//! quoted-key form just like `.aristo/index.toml`.
//!
//! Per canon-strategy.md §CS12, the cache **pins `(canon_id,
//! version)`**. Server-side patch bumps refresh cached entries
//! transparently; minor bumps trigger auto-unbind (handled by the
//! stamp version-migration path in PR #12, not here).
//!
//! ## Three buckets per annotation
//!
//! - **`pending_matches`**: surfaced but not yet reviewed. Stamp /
//!   critique populate; the review session moves entries to either
//!   accepted_matches or rejected_matches.
//! - **`accepted_matches`**: bound — the source has the canon prefix
//!   applied. Steady state after canon-accept.
//! - **`rejected_matches`**: explicit user rejection. Pinned by
//!   `text_hash`; once the annotation text changes, the rejection
//!   no longer suppresses re-evaluation (per L5 invalidation rules).
//!
//! ## Atomic write
//!
//! Writes go through a temp-then-rename dance (see [`write_atomic`])
//! so an interrupted write never leaves a half-written file. Reads
//! tolerate a missing file by returning an empty cache.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::PrefixTier;
use crate::index::AnnotationId;

/// Current on-disk schema version. Bump on any breaking shape change.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level `.aristo/canon-matches.toml` document.
///
/// `__meta__` carries the schema version + catalog snapshot tag +
/// last-fetched timestamp. Every other key is an annotation id
/// mapping to its [`CacheEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct CanonMatchesFile {
    #[serde(rename = "__meta__", default)]
    pub meta: CacheMeta,

    /// annotation_id → CacheEntry. Iteration order follows BTreeMap
    /// for deterministic serialization (load-bearing for clean git
    /// diffs since the file is committed by default).
    #[serde(flatten)]
    pub entries: BTreeMap<AnnotationId, CacheEntry>,
}

/// `[__meta__]` header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CacheMeta {
    /// Cache schema version (currently [`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Catalog snapshot tag from the most-recent server response
    /// (e.g., `"v0.2.0"`). Informational; per-entry version
    /// (`PendingMatch::version`) is the load-bearing cache key per
    /// canon-strategy.md §CS12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canon_version: Option<String>,
    /// RFC 3339 timestamp of the most-recent successful API call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetched: Option<String>,
}

impl Default for CacheMeta {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            canon_version: None,
            last_fetched: None,
        }
    }
}

/// Per-annotation cache entry. Up to three buckets — pending matches
/// (surfaced but not reviewed), accepted matches (bound; canonical
/// prefix applied to source), rejected matches (suppressed unless
/// annotation text changes).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CacheEntry {
    /// `text_hash` of the annotation's prose at the time of the
    /// last successful match. Cache hit when this matches the
    /// annotation's current `text_hash` AND every cached
    /// `(canon_id, version)` is still server-active.
    pub last_match_text_hash: String,
    /// RFC 3339 timestamp of the API call that produced this entry.
    pub canon_fetched_at: String,
    /// Matches that surfaced but haven't been reviewed yet. Critique
    /// session moves these to `accepted_matches` or `rejected_matches`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_matches: Vec<PendingMatch>,
    /// Bound matches — source has the canon prefix applied and the
    /// index entry records the binding. Steady state after accept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_matches: Vec<AcceptedMatch>,
    /// Rejected matches — pinned by `text_hash` so the same
    /// `(canon_id, text_hash)` doesn't re-surface until the
    /// annotation text changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_matches: Vec<RejectedMatch>,
}

/// A pending (surfaced but not yet reviewed) canon match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PendingMatch {
    pub canon_id: String,
    /// Per-entry version pinned at match time (per CS12).
    pub version: String,
    pub canonical_text: String,
    /// Catalog snapshot tag at match time. Informational only.
    pub canon_version: String,
    pub confidence: f64,
    pub prefix_tier: PrefixTier,
    /// `Some(_)` for `aristos:` tier; `None` for `kanon:` tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backed_by: Option<String>,
    /// Opaque server-issued binding handle. Carried verbatim from
    /// the `/canon/match` response so accept-time logic can route it
    /// into `BindingState::Bound { linked }`. **Phase 1 carve-out:**
    /// the field is `Option<String>` because the current dev/prod
    /// proxy doesn't emit `linked` yet — see
    /// [`canon::types::CanonMatch::linked`] for the full rationale and
    /// the Phase 2 plan that restores it to required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<String>,
    /// Review state — `"open"` until the user decides.
    pub disposition: Disposition,
    /// RFC 3339 timestamp of when stamp/critique surfaced this match.
    pub found_at: String,
    /// Which command surfaced this match (e.g., `"aristo stamp"` or
    /// `"aristo critique"`). Informational; useful for debugging.
    pub found_by: String,
}

impl Eq for PendingMatch {}

/// Review state of a `PendingMatch`. Open until the user decides
/// (accept / skip / reject). Rejection moves the match into
/// [`RejectedMatch`] — it doesn't linger here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    /// Surfaced but not yet reviewed.
    Open,
    /// User has decided "skip; re-surface next session" — pending
    /// stays open but the next critique session moves it back to
    /// the top of the queue.
    Skipped,
    /// User has accepted the match. The pending entry stays in
    /// `pending_matches[..]` with `disposition = "accepted"` only
    /// transiently — `aristo canon accept` immediately rewrites
    /// source, mutates the index, and moves the entry to
    /// `accepted_matches[..]`. A pending match observed in this
    /// state on disk means the apply step was interrupted; the
    /// next accept (or stamp) reconciles. See PR #7 (the accept
    /// path) for the atomic ordering contract.
    Accepted,
}

/// An accepted (bound) canon match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptedMatch {
    pub canon_id: String,
    pub version: String,
    pub canonical_text: String,
    pub canon_version: String,
    pub confidence: f64,
    pub prefix_tier: PrefixTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backed_by: Option<String>,
    /// RFC 3339 timestamp of acceptance (when the user clicked
    /// `[a]ccept` in the critique session).
    pub accepted_at: String,
    /// RFC 3339 timestamp of when the canon prefix was applied to
    /// source. In Phase 1 this is always equal to `accepted_at`
    /// because accept and bind happen atomically (per CS13's "no
    /// separate sync" decision).
    pub bound_at: String,
}

impl Eq for AcceptedMatch {}

/// An explicitly rejected canon match. Pinned by `text_hash` so the
/// same `(annotation_id, canon_id, text_hash)` tuple doesn't
/// re-surface until the annotation text changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RejectedMatch {
    pub canon_id: String,
    pub version: String,
    /// `text_hash` of the annotation prose at rejection time.
    /// Once the annotation text changes, the rejection no longer
    /// suppresses re-evaluation (per L5 invalidation rules).
    pub text_hash: String,
    pub rejected_at: String,
    /// Optional free-text rationale from the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ─── File I/O ──────────────────────────────────────────────────────────────

impl CanonMatchesFile {
    /// Read the canon-matches cache from `path`. Returns
    /// `Ok(default)` if the file doesn't exist (typical first-run
    /// case). Surfaces parse errors as [`io::Error::other`] so
    /// callers can propagate via `?`.
    pub fn read(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw)
                .map_err(|e| io::Error::other(format!("parse {}: {e}", path.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Write the canon-matches cache to `path` atomically (write to
    /// `<path>.tmp` then rename). Creates parent directories as
    /// needed.
    pub fn write_atomic(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::other(format!("serialize canon-matches: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, toml_text.as_bytes())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

// ─── Cache-hit semantics (per L5 invalidation rules) ──────────────────────

impl CacheEntry {
    /// Is this entry's cached data still authoritative for the
    /// given current annotation text-hash AND server-side
    /// per-entry version?
    ///
    /// L5 says the cache is a hit when:
    /// 1. The annotation's `text_hash` hasn't changed, AND
    /// 2. Every cached `(canon_id, version)` is still server-active.
    ///
    /// This helper covers condition (1) plus a per-match version
    /// check against the provided lookup. Callers pass a closure
    /// `is_version_active(canon_id, version) -> bool` so we don't
    /// take a dependency on a particular catalog representation.
    pub fn is_fresh_for(
        &self,
        current_text_hash: &str,
        is_version_active: impl Fn(&str, &str) -> bool,
    ) -> bool {
        if self.last_match_text_hash != current_text_hash {
            return false;
        }
        // Pending + accepted matches both pin (canon_id, version);
        // either set going stale invalidates the cache.
        let stale_in = |matches: &[(&str, &str)]| -> bool {
            !matches
                .iter()
                .all(|(canon_id, version)| is_version_active(canon_id, version))
        };
        let pending_pairs: Vec<(&str, &str)> = self
            .pending_matches
            .iter()
            .map(|m| (m.canon_id.as_str(), m.version.as_str()))
            .collect();
        if stale_in(&pending_pairs) {
            return false;
        }
        let accepted_pairs: Vec<(&str, &str)> = self
            .accepted_matches
            .iter()
            .map(|m| (m.canon_id.as_str(), m.version.as_str()))
            .collect();
        if stale_in(&accepted_pairs) {
            return false;
        }
        true
    }

    /// Is this `(canon_id, text_hash)` tuple already suppressed by
    /// an explicit user rejection? Used by stamp/critique to skip
    /// re-surfacing previously-rejected matches per L5.
    pub fn is_rejected(&self, canon_id: &str, text_hash: &str) -> bool {
        self.rejected_matches
            .iter()
            .any(|r| r.canon_id == canon_id && r.text_hash == text_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap_or_else(|e| panic!("parse {s:?}: {e}"))
    }

    fn sample_pending() -> PendingMatch {
        PendingMatch {
            canon_id: "cell_written_exactly_once_per_page_edit".into(),
            version: "v0.2.1".into(),
            canonical_text: "edit_page writes each cell exactly once".into(),
            canon_version: "v0.2.0".into(),
            confidence: 0.92,
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("specialized neural checker".into()),
            linked: Some("arta_a1b2c3d4".into()),
            disposition: Disposition::Open,
            found_at: "2026-06-15T09:14:22Z".into(),
            found_by: "aristo stamp".into(),
        }
    }

    fn sample_accepted() -> AcceptedMatch {
        AcceptedMatch {
            canon_id: "cell_written_exactly_once_per_page_edit".into(),
            version: "v0.2.1".into(),
            canonical_text: "edit_page writes each cell exactly once".into(),
            canon_version: "v0.2.0".into(),
            confidence: 1.0,
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("specialized neural checker".into()),
            accepted_at: "2026-06-15T09:20:00Z".into(),
            bound_at: "2026-06-15T09:20:00Z".into(),
        }
    }

    fn sample_rejected() -> RejectedMatch {
        RejectedMatch {
            canon_id: "some_unrelated_entry".into(),
            version: "v0.1.2".into(),
            text_hash: "blake3:c2f7a912".into(),
            rejected_at: "2026-06-13T11:48:00Z".into(),
            reason: Some("intentionally narrower than canon entry".into()),
        }
    }

    // ─── Schema round-trip ────────────────────────────────────────────────

    #[test]
    fn empty_file_round_trips() {
        let f = CanonMatchesFile::default();
        let s = toml::to_string(&f).unwrap();
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.meta.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn disposition_all_variants_round_trip_as_kebab_case() {
        // Three legal Dispositions; serialize-as-kebab-case is load-bearing
        // because `aristo canon accept` writes `"accepted"` to disk and the
        // critique apply path filters by string match in trycmd scenarios.
        for (variant, wire) in [
            (Disposition::Open, "\"open\""),
            (Disposition::Skipped, "\"skipped\""),
            (Disposition::Accepted, "\"accepted\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, wire, "variant {variant:?} should serialize as {wire}");
            let back: Disposition = serde_json::from_str(wire).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn pending_match_with_accepted_disposition_round_trips() {
        // PR #7's transient state: a pending match whose disposition is
        // `Accepted` but whose source/index/cache application is pending.
        let mut p = sample_pending();
        p.disposition = Disposition::Accepted;
        let mut f = CanonMatchesFile::default();
        f.entries.insert(
            aid("edit_page_cell_write_invariant"),
            CacheEntry {
                last_match_text_hash: "blake3:7f3a9e2c".into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![p],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );
        let s = toml::to_string(&f).unwrap();
        assert!(s.contains("disposition = \"accepted\""), "got: {s}");
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn pending_match_round_trips_with_aristos_tier() {
        let mut f = CanonMatchesFile::default();
        f.entries.insert(
            aid("edit_page_cell_write_invariant"),
            CacheEntry {
                last_match_text_hash: "blake3:7f3a9e2c".into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![sample_pending()],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );
        let s = toml::to_string(&f).unwrap();
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
        let entry = &back.entries[&aid("edit_page_cell_write_invariant")];
        assert_eq!(entry.pending_matches[0].prefix_tier, PrefixTier::Aristos);
        assert_eq!(
            entry.pending_matches[0].backed_by.as_deref(),
            Some("specialized neural checker")
        );
    }

    #[test]
    fn kanon_tier_pending_omits_backed_by_in_serialized_form() {
        let mut p = sample_pending();
        p.prefix_tier = PrefixTier::Kanon;
        p.backed_by = None;
        let mut f = CanonMatchesFile::default();
        f.entries.insert(
            aid("foo"),
            CacheEntry {
                last_match_text_hash: "blake3:abc".into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![p],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );
        let s = toml::to_string(&f).unwrap();
        // No `backed_by =` key should appear in the serialized form
        // for the kanon: tier entry.
        assert!(!s.contains("backed_by"), "got: {s}");
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back.entries[&aid("foo")].pending_matches[0].backed_by, None);
    }

    #[test]
    fn canon_prefixed_keys_round_trip() {
        // Quoted-key form for canon-bound ids (aristos: + kanon:).
        let mut f = CanonMatchesFile::default();
        f.entries.insert(
            aid("aristos:cell_written_exactly_once_per_page_edit"),
            CacheEntry {
                last_match_text_hash: "blake3:9d4e2f01".into(),
                canon_fetched_at: "2026-06-15T09:30:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![sample_accepted()],
                rejected_matches: vec![],
            },
        );
        f.entries.insert(
            aid("kanon:checkout_total_non_negative"),
            CacheEntry {
                last_match_text_hash: "blake3:a4f721e8".into(),
                canon_fetched_at: "2026-06-15T09:30:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![AcceptedMatch {
                    canon_id: "checkout_total_non_negative".into(),
                    version: "v0.1.0".into(),
                    canonical_text: "checkout total is non-negative".into(),
                    canon_version: "v0.2.0".into(),
                    confidence: 0.94,
                    prefix_tier: PrefixTier::Kanon,
                    backed_by: None,
                    accepted_at: "2026-06-14T17:02:11Z".into(),
                    bound_at: "2026-06-14T17:02:11Z".into(),
                }],
                rejected_matches: vec![],
            },
        );
        let s = toml::to_string(&f).unwrap();
        assert!(s.contains("\"aristos:cell_written"), "got: {s}");
        assert!(s.contains("\"kanon:checkout_total"), "got: {s}");
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn rejected_match_round_trip() {
        let mut f = CanonMatchesFile::default();
        f.entries.insert(
            aid("my_local_invariant"),
            CacheEntry {
                last_match_text_hash: "blake3:c2f7a912".into(),
                canon_fetched_at: "2026-06-13T10:15:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![sample_rejected()],
            },
        );
        let s = toml::to_string(&f).unwrap();
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn meta_block_round_trips_with_canon_version() {
        let f = CanonMatchesFile {
            meta: CacheMeta {
                schema_version: 1,
                canon_version: Some("v0.2.0".into()),
                last_fetched: Some("2026-06-15T09:14:22Z".into()),
            },
            entries: BTreeMap::new(),
        };
        let s = toml::to_string(&f).unwrap();
        assert!(s.contains("schema_version = 1"));
        assert!(s.contains("canon_version = \"v0.2.0\""));
        let back: CanonMatchesFile = toml::from_str(&s).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn unknown_field_inside_pending_match_rejected() {
        // deny_unknown_fields on PendingMatch should catch
        // server-side schema drift.
        let toml_text = r#"
[__meta__]
schema_version = 1

[foo]
last_match_text_hash = "blake3:x"
canon_fetched_at = "2026-06-15T09:14:22Z"

[[foo.pending_matches]]
canon_id = "x"
version = "v0.1.0"
canonical_text = "y"
canon_version = "v0.2.0"
confidence = 0.9
prefix_tier = "aristos:"
linked = "arta_x"
disposition = "open"
found_at = "2026-06-15T09:14:22Z"
found_by = "aristo stamp"
unknown_field = "should reject"
"#;
        let result: Result<CanonMatchesFile, _> = toml::from_str(toml_text);
        assert!(result.is_err(), "expected deny_unknown_fields rejection");
    }

    // ─── L5 worked-example round-trip ────────────────────────────────────
    //
    // The locked sample lives at
    // `../aretta-sdk/docs/mockups/13-canon-and-matching/sample-matches.toml`
    // (meta-workspace sibling). `include_str!` can't reach it via
    // relative path because Cargo resolves to the canonical file
    // location, and the meta-workspace is a sibling-of-aristo rather
    // than a parent. The test below reconstructs the worked example
    // inline so this file is byte-aligned with the locked design
    // sample (each future schema change requires updating both).

    #[test]
    fn worked_example_matches_locked_sample_shape() {
        let raw = r#"
[__meta__]
schema_version = 1
canon_version  = "v0.2.0"
last_fetched   = "2026-06-15T09:14:22Z"

[edit_page_cell_write_invariant]
last_match_text_hash = "blake3:7f3a9e2c..."
canon_fetched_at     = "2026-06-15T09:14:22Z"

[[edit_page_cell_write_invariant.pending_matches]]
canon_id       = "cell_written_exactly_once_per_page_edit"
version        = "v0.2.1"
canonical_text = "edit_page writes each cell exactly once"
canon_version  = "v0.2.0"
confidence     = 0.92
prefix_tier    = "aristos:"
backed_by      = "specialized neural checker"
linked         = "arta_a1b2c3d4..."
disposition    = "open"
found_at       = "2026-06-15T09:14:22Z"
found_by       = "aristo stamp"

["aristos:cell_written_exactly_once_per_page_edit"]
last_match_text_hash = "blake3:9d4e2f01..."
canon_fetched_at     = "2026-06-15T09:30:00Z"

[["aristos:cell_written_exactly_once_per_page_edit".accepted_matches]]
canon_id       = "cell_written_exactly_once_per_page_edit"
version        = "v0.2.1"
canonical_text = "edit_page writes each cell exactly once"
canon_version  = "v0.2.0"
confidence     = 1.0
prefix_tier    = "aristos:"
backed_by      = "specialized neural checker"
accepted_at    = "2026-06-15T09:20:00Z"
bound_at       = "2026-06-15T09:20:00Z"

["kanon:checkout_total_non_negative"]
last_match_text_hash = "blake3:a4f721e8..."
canon_fetched_at     = "2026-06-15T09:30:00Z"

[["kanon:checkout_total_non_negative".accepted_matches]]
canon_id       = "checkout_total_non_negative"
version        = "v0.1.0"
canonical_text = "checkout total is non-negative"
canon_version  = "v0.2.0"
confidence     = 0.94
prefix_tier    = "kanon:"
accepted_at    = "2026-06-14T17:02:11Z"
bound_at       = "2026-06-14T17:02:11Z"

[my_local_invariant]
last_match_text_hash = "blake3:c2f7a912..."
canon_fetched_at     = "2026-06-13T10:15:00Z"

[[my_local_invariant.rejected_matches]]
canon_id    = "some_unrelated_entry"
version     = "v0.1.2"
text_hash   = "blake3:c2f7a912..."
rejected_at = "2026-06-13T11:48:00Z"
reason      = "intentionally narrower than canon entry"
"#;
        let parsed: CanonMatchesFile =
            toml::from_str(raw).unwrap_or_else(|e| panic!("locked sample shape must parse: {e}"));

        // All four worked examples present.
        assert!(parsed
            .entries
            .contains_key(&aid("edit_page_cell_write_invariant")));
        assert!(parsed
            .entries
            .contains_key(&aid("aristos:cell_written_exactly_once_per_page_edit")));
        assert!(parsed
            .entries
            .contains_key(&aid("kanon:checkout_total_non_negative")));
        assert!(parsed.entries.contains_key(&aid("my_local_invariant")));
        assert_eq!(parsed.meta.canon_version.as_deref(), Some("v0.2.0"));

        // kanon: tier entry has no backed_by — the file omits the
        // key entirely (TOML has no null; we treat absence as None).
        let kanon_entry = &parsed.entries[&aid("kanon:checkout_total_non_negative")];
        assert_eq!(kanon_entry.accepted_matches[0].backed_by, None);
        assert_eq!(
            kanon_entry.accepted_matches[0].prefix_tier,
            PrefixTier::Kanon
        );
    }

    // ─── Atomic I/O ──────────────────────────────────────────────────────

    #[test]
    fn read_missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("never-existed.toml");
        let f = CanonMatchesFile::read(&p).unwrap();
        assert_eq!(f, CanonMatchesFile::default());
    }

    #[test]
    fn read_then_write_round_trips_through_disk() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join(".aristo/canon-matches.toml");

        let mut original = CanonMatchesFile::default();
        original.entries.insert(
            aid("foo"),
            CacheEntry {
                last_match_text_hash: "blake3:x".into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![sample_pending()],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );
        original.meta.canon_version = Some("v0.2.0".into());

        original.write_atomic(&p).unwrap();
        let loaded = CanonMatchesFile::read(&p).unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn write_atomic_creates_parent_directory() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("deep/nested/.aristo/canon-matches.toml");
        assert!(!p.parent().unwrap().exists());
        CanonMatchesFile::default().write_atomic(&p).unwrap();
        assert!(p.exists());
    }

    #[test]
    fn write_atomic_replaces_existing_file() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("canon-matches.toml");
        fs::write(&p, b"junk that should be overwritten").unwrap();
        let f = CanonMatchesFile {
            meta: CacheMeta {
                schema_version: 1,
                canon_version: Some("v0.2.0".into()),
                last_fetched: None,
            },
            entries: BTreeMap::new(),
        };
        f.write_atomic(&p).unwrap();
        let loaded = CanonMatchesFile::read(&p).unwrap();
        assert_eq!(loaded, f);
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file_on_success() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("canon-matches.toml");
        CanonMatchesFile::default().write_atomic(&p).unwrap();
        let tmp_path = p.with_extension("toml.tmp");
        assert!(!tmp_path.exists(), "tmp file should have been renamed");
    }

    #[test]
    fn malformed_file_returns_io_error_not_default() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("canon-matches.toml");
        fs::write(&p, b"this is not TOML = = =").unwrap();
        let err = CanonMatchesFile::read(&p).unwrap_err();
        assert!(err.to_string().contains("parse"), "got: {err}");
    }

    // ─── Cache-hit semantics ──────────────────────────────────────────────

    #[test]
    fn cache_hit_when_text_hash_matches_and_versions_active() {
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches: vec![sample_pending()],
            accepted_matches: vec![],
            rejected_matches: vec![],
        };
        // sample_pending() has canon_id="cell_written…" version="v0.2.1"
        let is_active = |_cid: &str, _v: &str| true;
        assert!(entry.is_fresh_for("blake3:x", is_active));
    }

    #[test]
    fn cache_miss_when_text_hash_changes() {
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches: vec![],
            accepted_matches: vec![],
            rejected_matches: vec![],
        };
        assert!(!entry.is_fresh_for("blake3:DIFFERENT", |_, _| true));
    }

    #[test]
    fn cache_miss_when_pending_match_version_no_longer_active() {
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches: vec![sample_pending()], // version v0.2.1
            accepted_matches: vec![],
            rejected_matches: vec![],
        };
        // Server says v0.2.1 no longer active (patch or minor bump).
        let is_active = |_cid: &str, version: &str| version != "v0.2.1";
        assert!(!entry.is_fresh_for("blake3:x", is_active));
    }

    #[test]
    fn cache_miss_when_accepted_match_version_no_longer_active() {
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches: vec![],
            accepted_matches: vec![sample_accepted()], // version v0.2.1
            rejected_matches: vec![],
        };
        let is_active = |_cid: &str, version: &str| version != "v0.2.1";
        assert!(!entry.is_fresh_for("blake3:x", is_active));
    }

    #[test]
    fn rejected_matches_dont_affect_freshness() {
        // Rejected entries are independent of cache freshness —
        // they suppress re-surfacing via is_rejected, not via
        // is_fresh_for.
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-15T09:14:22Z".into(),
            pending_matches: vec![],
            accepted_matches: vec![],
            rejected_matches: vec![sample_rejected()],
        };
        // Even with the rejected_match's canon_id "no longer active",
        // freshness is unaffected — it's an annotation-level state,
        // not a pending/accepted state.
        let is_active = |_cid: &str, _v: &str| false;
        assert!(entry.is_fresh_for("blake3:x", is_active));
    }

    #[test]
    fn is_rejected_matches_canon_id_and_text_hash() {
        let entry = CacheEntry {
            last_match_text_hash: "blake3:x".into(),
            canon_fetched_at: "2026-06-13T10:15:00Z".into(),
            pending_matches: vec![],
            accepted_matches: vec![],
            rejected_matches: vec![sample_rejected()],
        };
        // sample_rejected: canon_id="some_unrelated_entry", text_hash="blake3:c2f7a912"
        assert!(entry.is_rejected("some_unrelated_entry", "blake3:c2f7a912"));
        // Different canon_id: not rejected.
        assert!(!entry.is_rejected("other_id", "blake3:c2f7a912"));
        // Different text_hash (annotation text changed): no longer rejected.
        assert!(!entry.is_rejected("some_unrelated_entry", "blake3:DIFFERENT"));
    }
}
