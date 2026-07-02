//! Shared canon-match step. Used by `aristo stamp` (PR #5) and
//! `aristo critique` (PR #6) — same client selection, same cache
//! update, same graceful-degradation policy. Different thresholds
//! and a different `found_by` label.
//!
//! ## Client selection
//!
//! The right [`CanonClient`] depends on tier + test mode:
//!
//! | Condition | Client | Behavior |
//! |---|---|---|
//! | `ARISTO_CANON_FIXTURE` set | [`MockCanonClient`] | reads canned TOML fixtures (test mode) |
//! | `[canon] enabled = false` | [`NoopCanonClient`] | silent skip — opt-out for regulated buyers |
//! | `--skip-canon` flag | [`NoopCanonClient`] | silent skip — per-invocation opt-out |
//! | Auth token resolves | [`HttpCanonClient`] | real API call (Pro / Enterprise) |
//! | No token resolved | [`NoopCanonClient`] | free-tier path; runner prints upgrade nudge |
//!
//! `ARETTA_API_URL` env var overrides the production base URL
//! (for staging + integration tests against a local TCP listener).
//!
//! ## L5 cache-skip policy
//!
//! The runner batches only annotations whose cache entry is stale:
//!
//! - **First-run** (no cached entry) — always batch.
//! - **Text drift** (`text_hash` changed since last match) — batch.
//! - **`--refresh-canon`** — invalidate cache, batch all.
//! - **Previously rejected** (canon_id + text_hash in
//!   `rejected_matches`) — skip; rejection lifts only on text change.
//!
//! Annotations with a fresh cache hit produce no API traffic. If
//! every annotation is a cache hit, the API call is skipped
//! entirely — a "no new findings" run is fully offline.

use std::time::SystemTime;

use aristo_core::canon::{
    AnnotationMatchInput, CacheEntry, CanonClient, CanonError, CanonMatchRequest,
    CanonMatchResponse, CanonMatchesFile, Disposition, HttpCanonClient, MockCanonClient,
    NoopCanonClient, PendingMatch,
};
use aristo_core::config::CanonConfig;
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile};

use crate::commands::canon::suggestions as suggestions_mod;
use crate::pipeline::queue;
use crate::{CliError, CliResult, Workspace};

/// What happened in the canon-step. Stamp/critique render this for
/// the user; the variant determines the output shape (see Flow 1 /
/// Flow 2 / Flow 6 in cli-sessions.md).
#[derive(Debug)]
pub(crate) enum CanonStepOutcome {
    /// API call succeeded; cache updated; `findings_added` new
    /// pending matches surfaced this run.
    Ok {
        findings_added: usize,
        canon_version: Option<String>,
    },
    /// No API call needed — every annotation had a fresh cache hit.
    /// Cached matches (if any) remain visible to `aristo critique`.
    CacheHit { existing_pending: usize },
    /// `[canon] enabled = false` in `aristo.toml`. Silent skip
    /// (regulated-buyer / air-gapped opt-out per CS5).
    DisabledByConfig,
    /// `--skip-canon` flag on the invocation. Per-invocation
    /// opt-out; same silence as `DisabledByConfig`.
    SkippedByFlag,
    /// Free-tier user (no auth token + not in test mode). Runner
    /// surfaces a one-line upgrade nudge; cached matches (if any)
    /// from a prior paid session stay readable but no new matches
    /// are surfaced.
    FreeTier { annotations_skipped: usize },
    /// API call failed (timeout, network, auth, server). Cached
    /// matches retained per L3's graceful-degradation policy.
    /// `failed_for` records how many annotations would have been
    /// batched in this run so the warning can be specific.
    Degraded {
        error: CanonError,
        failed_for: usize,
    },
}

/// Per-invocation runner config. Stamp passes `threshold = config.threshold_stamp`
/// + `found_by = "aristo stamp"`; critique passes `threshold = config.threshold_critique`
/// + `found_by = "aristo critique"`.
pub(crate) struct RunnerArgs<'a> {
    pub(crate) ws: &'a Workspace,
    pub(crate) index: &'a IndexFile,
    pub(crate) config: &'a CanonConfig,
    pub(crate) threshold: f64,
    pub(crate) skip_flag: bool,
    pub(crate) refresh_flag: bool,
    pub(crate) found_by: &'static str,
}

/// Execute the canon-match step end-to-end:
/// 1. Build the right [`CanonClient`] for the tier / test mode.
/// 2. Read the existing cache.
/// 3. Determine the annotation batch (cache-misses + drift + refresh).
/// 4. Call the API (if batch is non-empty).
/// 5. Update + atomic-write the cache.
/// 6. Return a [`CanonStepOutcome`] for stamp/critique to print.
pub(crate) fn run_canon_step(args: RunnerArgs) -> CliResult<CanonStepOutcome> {
    // ── Per-invocation opt-outs ────────────────────────────────────────────
    if args.skip_flag {
        return Ok(CanonStepOutcome::SkippedByFlag);
    }
    if !args.config.enabled {
        return Ok(CanonStepOutcome::DisabledByConfig);
    }

    // ── Read existing cache (or empty if first run) ────────────────────────
    let cache_path = args.ws.canon_matches_path();
    let mut cache = CanonMatchesFile::read(&cache_path).map_err(|e| CliError::Other {
        message: format!("read {}: {e}", cache_path.display()),
        exit_code: 1,
    })?;

    // ── Build the canon client. NoopCanonClient is the free-tier
    //    + missing-token path; the caller branches on the outcome to
    //    print the nudge. ────────────────────────────────────────────────
    let (client, is_free_tier) = build_client(args.config);

    // ── Collect annotations needing a fresh match ──────────────────────────
    let batch = collect_batch(args.index, &cache, args.refresh_flag);
    if batch.is_empty() {
        let existing_pending = count_pending(&cache);
        return Ok(CanonStepOutcome::CacheHit { existing_pending });
    }

    // ── Free-tier short-circuit: no API call, just surface the nudge ──────
    if is_free_tier {
        return Ok(CanonStepOutcome::FreeTier {
            annotations_skipped: batch.len(),
        });
    }

    // ── Call /canon/match ─────────────────────────────────────────────────
    let req = CanonMatchRequest {
        annotations: batch
            .iter()
            .map(|b| AnnotationMatchInput {
                annotation_text: b.text.clone(),
                applies_to: b.applies_to.clone(),
            })
            .collect(),
        confidence_threshold: args.threshold,
        // §17: opt in to the proof-tree suggestions channel. The server
        // attaches `suggestions[]` aligned by annotation index; we route
        // them into the canon-suggestions queue after the primary merge.
        include_suggestions: true,
    };
    let response = match client.match_annotations(&req) {
        Ok(r) => r,
        Err(e) => {
            return Ok(CanonStepOutcome::Degraded {
                error: e,
                failed_for: batch.len(),
            });
        }
    };

    // ── Merge response into cache + write atomically ──────────────────────
    let findings_added = merge_response_into_cache(&mut cache, &batch, &response, args.found_by);
    cache.meta.canon_version = Some(response.canon_version.clone());
    cache.meta.last_fetched = Some(response.matched_at.clone());
    cache
        .write_atomic(&cache_path)
        .map_err(|e| CliError::Other {
            message: format!("write {}: {e}", cache_path.display()),
            exit_code: 1,
        })?;

    // ── §17: route proof-tree suggestions into the canon-suggestions
    //    queue. Dedup ② (vs local index/cache/rejection-log state) +
    //    dedup ③ (collapse clusters sharing an objective) happen inside
    //    the router. Local state is read AFTER the primary merge so a
    //    sibling that just landed as a pending primary is filtered out. ──
    if let Some(suggestions) = &response.suggestions {
        let qdir = queue::QueueDir::for_pipeline(args.ws, suggestions_mod::PIPELINE);
        let local = suggestions_mod::local_state(args.ws, &cache)?;
        suggestions_mod::route_suggestions_into_queue(&qdir, suggestions, &local, &now_rfc3339())?;
    }

    Ok(CanonStepOutcome::Ok {
        findings_added,
        canon_version: Some(response.canon_version),
    })
}

// ─── Client selection ──────────────────────────────────────────────────────

/// Returns `(client, is_free_tier)`. `is_free_tier` is true only
/// when the client is a Noop *because* of missing auth — `[canon]
/// enabled = false` and `--skip-canon` are handled upstream.
#[aristo::intent(
    "Client selection order is load-bearing: ARISTO_CANON_FIXTURE \
     wins outright (test mode beats everything, including auth), \
     then auth-token resolution decides between HttpCanonClient and \
     the free-tier Noop. Reversing — e.g. checking auth first — \
     would make integration tests need a fake token to work, \
     coupling test setup to the auth substrate unnecessarily.",
    verify = "test",
    id = "canon_client_selection_test_mode_wins"
)]
fn build_client(_config: &CanonConfig) -> (Box<dyn CanonClient>, bool) {
    // Test mode: ARISTO_CANON_FIXTURE always wins, even over auth.
    // Lets integration tests run end-to-end without setting up a
    // token.
    if let Some(mock) = MockCanonClient::from_env() {
        return (Box::new(mock), false);
    }

    // Production / staging: resolve auth token. If unresolved →
    // free tier (Noop, with the nudge).
    match aristo_core::auth::resolve_full() {
        Ok(creds) => {
            let base_url = crate::data_plane::resolve_base(&creds.server);
            (
                Box::new(HttpCanonClient::new(base_url, &creds.token)),
                false,
            )
        }
        Err(_) => (Box::new(NoopCanonClient), true),
    }
}

// ─── Batch collection (L5 cache-skip policy) ───────────────────────────────

#[derive(Debug)]
struct BatchEntry {
    id: AnnotationId,
    text: String,
    text_hash: String,
    applies_to: Vec<String>,
}

#[aristo::intent(
    "An annotation is added to the canon-match batch when (a) the \
     user passed --refresh-canon, OR (b) no cached entry exists yet, \
     OR (c) the cached entry's last_match_text_hash differs from the \
     current annotation text_hash. A fresh cache hit produces no API \
     traffic — load-bearing for the daily-loop UX where most stamps \
     touch nothing canon-relevant.",
    verify = "test",
    id = "canon_batch_collection_honors_l5_cache_skip"
)]
fn collect_batch(index: &IndexFile, cache: &CanonMatchesFile, refresh: bool) -> Vec<BatchEntry> {
    let mut batch = Vec::new();
    for (id, entry) in &index.entries {
        // Only Intent entries are canon-matchable per L3. Assume
        // entries are background facts, not invariants we'd surface
        // canon entries for.
        let intent = match entry {
            IndexEntry::Intent(i) => i,
            IndexEntry::Assume(_) => continue,
        };
        // Skip ids already in a canon-bound namespace — they're
        // bound to a canon entry already; the version-migration
        // path (PR #12) handles their refresh.
        if id.is_canon_bound() {
            continue;
        }

        let text_hash = intent.text_hash.as_str().to_string();
        let needs_match = refresh
            || match cache.entries.get(id) {
                None => true,
                Some(cached) => cached.last_match_text_hash != text_hash,
            };
        if !needs_match {
            continue;
        }

        batch.push(BatchEntry {
            id: id.clone(),
            text: intent.text.clone(),
            text_hash,
            applies_to: applies_to_from_site(&intent.site),
        });
    }
    batch
}

/// Crude `applies_to` derivation from the entry's `site` string
/// (e.g., `"fn foo (line 12)"`, `"struct Bar (line 8)"`). The first
/// whitespace-separated token is the surface — matches the
/// `AnnotationMatchInput::applies_to` server contract.
fn applies_to_from_site(site: &str) -> Vec<String> {
    let head = site.split_whitespace().next().unwrap_or("");
    if head.is_empty() {
        return Vec::new();
    }
    // Map syn-surface keywords to the canon contract's set.
    match head {
        "fn" | "method" | "mod" | "struct" | "enum" | "trait" | "type" => {
            vec![head.to_string()]
        }
        // `impl` is special — the walker emits the site as either
        // `"impl X for Y"` (annotation on the impl block itself) or
        // `"impl X for Y::method_name"` (annotation on a fn/method
        // within the impl). For the latter, the kind we send to
        // `/canon/match` must be `method` — canon entries declare
        // their kinds against the annotated item, not its containing
        // block, so an `applies_to: [fn, method]` entry needs to see
        // `["method"]` from us, not `["impl"]`, to pass the server's
        // intersection filter.
        "impl" => {
            if site.contains("::") {
                vec!["method".to_string()]
            } else {
                vec!["impl".to_string()]
            }
        }
        // Unknown surface — still send something so the server can
        // filter; if the canon entry has no `applies_to` constraint,
        // it'll match regardless.
        other => vec![other.to_string()],
    }
}

// ─── Response → cache merge ────────────────────────────────────────────────

#[aristo::intent(
    "Merging match response into cache is per-annotation idempotent: \
     each batched annotation's candidate list replaces ONLY that \
     annotation's `pending_matches`; `accepted_matches` and \
     `rejected_matches` for the same annotation are untouched (user \
     decisions survive). A regression that overwrote accepted/rejected \
     here would silently undo the user's review work on every stamp.",
    verify = "test",
    id = "canon_merge_response_preserves_user_decisions"
)]
fn merge_response_into_cache(
    cache: &mut CanonMatchesFile,
    batch: &[BatchEntry],
    response: &CanonMatchResponse,
    found_by: &str,
) -> usize {
    let now = now_rfc3339();
    let mut total_added = 0usize;

    for (i, entry) in batch.iter().enumerate() {
        let candidates = response.results.get(i).cloned().unwrap_or_default();
        // Suppress any candidates already in this annotation's
        // rejected_matches for the current text_hash. (Per L5: a
        // rejection lifts only when text changes; since we're
        // batching the current text, current rejections are still
        // valid.)
        let cached_entry = cache
            .entries
            .entry(entry.id.clone())
            .or_insert_with(|| CacheEntry {
                last_match_text_hash: entry.text_hash.clone(),
                canon_fetched_at: now.clone(),
                pending_matches: Vec::new(),
                accepted_matches: Vec::new(),
                rejected_matches: Vec::new(),
            });

        let pending: Vec<PendingMatch> = candidates
            .into_iter()
            .filter(|c| !cached_entry.is_rejected(&c.canon_id, &entry.text_hash))
            .map(|c| {
                // P-008 carry (SLICE23-SPEC aristo item 2): keep the
                // whole verification block — coverage level, routed
                // test binaries, and the optional instrumentation
                // bundle — so it survives `canon accept` into
                // `accepted_matches` and later drives the S2 presence
                // probe + coverage-integrity check offline. JSON
                // nulls inside the bundle's verbatim Values must be
                // stripped first: the cache is TOML, and TOML cannot
                // represent null (a stray null would brick every
                // subsequent cache write).
                let mut verification = c.verification;
                if let Some(bundle) = verification.instrumentation.as_mut() {
                    aristo_core::canon::sanitize_bundle_for_persistence(bundle);
                }
                PendingMatch {
                    canon_id: c.canon_id,
                    version: c.version,
                    canonical_text: c.canonical_text,
                    canon_version: response.canon_version.clone(),
                    confidence: c.confidence,
                    prefix_tier: c.prefix_tier,
                    backed_by: c.backed_by,
                    linked: c.linked,
                    verification: Some(verification),
                    disposition: Disposition::Open,
                    found_at: now.clone(),
                    found_by: found_by.to_string(),
                }
            })
            .collect();

        total_added += pending.len();

        // Replace pending; keep accepted + rejected.
        cached_entry.last_match_text_hash = entry.text_hash.clone();
        cached_entry.canon_fetched_at = now.clone();
        cached_entry.pending_matches = pending;
    }

    total_added
}

fn count_pending(cache: &CanonMatchesFile) -> usize {
    cache
        .entries
        .values()
        .map(|e| e.pending_matches.len())
        .sum()
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    let _ = SystemTime::now();
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

// ─── User-facing summary printers ──────────────────────────────────────────

/// Render the canon-step outcome for `aristo stamp` per Flow 1/2/6
/// in cli-sessions.md.
pub(crate) fn print_stamp_summary(
    outcome: &CanonStepOutcome,
    cache: &CanonMatchesFile,
    ws: &Workspace,
) {
    match outcome {
        CanonStepOutcome::Ok {
            findings_added,
            canon_version,
        } => {
            let version_str = canon_version
                .as_deref()
                .map(|v| format!(", canon {v}"))
                .unwrap_or_default();
            println!("→ canon-match: {findings_added} new finding(s){version_str}.");
            print_pending_matches(cache);
        }
        CanonStepOutcome::CacheHit { existing_pending } => {
            if *existing_pending == 0 {
                println!("→ canon-match: no annotations need a fresh match.");
            } else {
                println!(
                    "→ canon-match: cache hit ({existing_pending} pending finding(s) still open)."
                );
                println!(
                    "    review with `aristo critique --apply-findings` or `aristo critique --filter id=<id>`"
                );
            }
        }
        CanonStepOutcome::DisabledByConfig => {
            println!(
                "→ canon-match: skipped (disabled via aristo.toml `[canon] enabled = false`)."
            );
        }
        CanonStepOutcome::SkippedByFlag => {
            println!("→ canon-match: skipped (`--skip-canon`).");
        }
        CanonStepOutcome::FreeTier {
            annotations_skipped,
        } => {
            println!("→ canon-match: skipped (Pro feature).");
            println!(
                "    note: canon matching is a Pro feature. {annotations_skipped} \
                 annotation(s) could have matched."
            );
            println!(
                "    Run `aristo auth login` to start a trial, or `aristo status` for details."
            );
        }
        CanonStepOutcome::Degraded { error, failed_for } => {
            let _ = ws; // path may surface in future detail lines
            println!("→ canon-match: skipped ({error}). cached matches retained.");
            if *failed_for > 0 {
                println!(
                    "    note: {failed_for} annotation(s) skipped this run; \
                     {} cached match(es) still valid.",
                    count_pending(cache)
                );
            }
        }
    }
}

fn print_pending_matches(cache: &CanonMatchesFile) {
    for (id, entry) in &cache.entries {
        for m in &entry.pending_matches {
            if !matches!(m.disposition, Disposition::Open) {
                continue;
            }
            let tier_label = match m.prefix_tier {
                aristo_core::canon::PrefixTier::Aristos => "aristos: tier",
                aristo_core::canon::PrefixTier::Kanon => "kanon: tier",
            };
            println!(
                "    {id}  → {canon_id} {version} (conf {conf:.2}, {tier_label})",
                canon_id = m.canon_id,
                version = m.version,
                conf = m.confidence,
            );
            if let Some(backed_by) = &m.backed_by {
                println!("      backed by: {backed_by}");
            }
            println!("      review with `aristo critique --filter id={id}`");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::canon::types::{
        CanonMatch, CanonMatchResponse, PrefixTier, VerificationMetadata,
    };
    use aristo_core::index::{
        AnnotationId, IndexEntry, IntentEntry, Meta, Sha256, Status, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn aid(s: &str) -> AnnotationId {
        AnnotationId::parse(s).unwrap()
    }

    fn sha(seed: char) -> Sha256 {
        let body: String = std::iter::repeat_n(seed, 64).collect();
        Sha256::parse(&format!("sha256:{body}")).unwrap()
    }

    fn intent_entry(text: &str, text_hash: Sha256, site: &str) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: text.into(),
            verify: VerifyLevel::Method(VerifyMethod::Neural),
            status: Status::Unknown,
            text_hash,
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: site.into(),
            covered_region: aristo_core::index::CoveredRegion::Function,
            binding: aristo_core::index::BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        })
    }

    fn empty_index() -> IndexFile {
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: BTreeMap::new(),
        }
    }

    // ─── collect_batch: cache-skip policy ─────────────────────────────────

    #[test]
    fn collect_batch_first_run_includes_every_intent() {
        let mut index = empty_index();
        index.entries.insert(
            aid("alpha"),
            intent_entry("text alpha", sha('a'), "fn alpha (line 1)"),
        );
        index.entries.insert(
            aid("beta"),
            intent_entry("text beta", sha('b'), "fn beta (line 2)"),
        );

        let cache = CanonMatchesFile::default();
        let batch = collect_batch(&index, &cache, false);
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn collect_batch_skips_annotations_with_cache_hit() {
        let mut index = empty_index();
        let text_hash = sha('a');
        index.entries.insert(
            aid("alpha"),
            intent_entry("text alpha", text_hash.clone(), "fn alpha (line 1)"),
        );
        index.entries.insert(
            aid("beta"),
            intent_entry("text beta", sha('b'), "fn beta (line 2)"),
        );

        let mut cache = CanonMatchesFile::default();
        // alpha is cached with the matching text_hash → skip.
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: text_hash.as_str().into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );

        let batch = collect_batch(&index, &cache, false);
        assert_eq!(batch.len(), 1, "only beta should be batched");
        assert_eq!(batch[0].id, aid("beta"));
    }

    #[test]
    fn collect_batch_includes_drifted_annotation() {
        let mut index = empty_index();
        index.entries.insert(
            aid("alpha"),
            intent_entry("alpha v2", sha('a'), "fn alpha (line 1)"),
        );

        let mut cache = CanonMatchesFile::default();
        // Cache has alpha with a DIFFERENT text_hash → drift.
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: "sha256:DIFFERENT".into(),
                canon_fetched_at: "2026-06-14T09:14:22Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );

        let batch = collect_batch(&index, &cache, false);
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn collect_batch_refresh_flag_invalidates_all_cache_hits() {
        let mut index = empty_index();
        let text_hash = sha('a');
        index.entries.insert(
            aid("alpha"),
            intent_entry("text alpha", text_hash.clone(), "fn alpha (line 1)"),
        );
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: text_hash.as_str().into(),
                canon_fetched_at: "2026-06-15T09:14:22Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );

        // Without refresh: cache hit, batch is empty.
        assert!(collect_batch(&index, &cache, false).is_empty());
        // With refresh: cache hit ignored, batch includes alpha.
        assert_eq!(collect_batch(&index, &cache, true).len(), 1);
    }

    #[test]
    fn collect_batch_excludes_canon_bound_ids() {
        let mut index = empty_index();
        index.entries.insert(
            aid("aristos:already_bound"),
            intent_entry("x", sha('a'), "fn x (line 1)"),
        );
        index.entries.insert(
            aid("kanon:also_bound"),
            intent_entry("y", sha('b'), "fn y (line 2)"),
        );
        index.entries.insert(
            aid("local_unbound"),
            intent_entry("z", sha('c'), "fn z (line 3)"),
        );
        let cache = CanonMatchesFile::default();
        let batch = collect_batch(&index, &cache, false);
        assert_eq!(batch.len(), 1, "only local id is batched");
        assert_eq!(batch[0].id, aid("local_unbound"));
    }

    // ─── applies_to_from_site ──────────────────────────────────────────────

    #[test]
    fn applies_to_extracts_first_token_from_site() {
        assert_eq!(applies_to_from_site("fn foo (line 12)"), vec!["fn"]);
        assert_eq!(applies_to_from_site("struct Bar (line 8)"), vec!["struct"]);
        assert_eq!(
            applies_to_from_site("method Baz::run (line 4)"),
            vec!["method"]
        );
    }

    #[test]
    fn applies_to_empty_site_returns_empty() {
        let v: Vec<String> = applies_to_from_site("");
        assert!(v.is_empty());
    }

    #[test]
    fn applies_to_for_fn_inside_impl_resolves_to_method() {
        // Annotations on a `fn` inside an `impl X for Y` block have
        // site = "impl X for Y::method_name (line N)". The kind we
        // send to /canon/match must be `method`, not `impl` — canon
        // entries declare applies_to against the annotated item, not
        // its enclosing block. Regression: when this returned ["impl"],
        // canon entries with applies_to:[fn,method] were filtered out
        // server-side and stamp reported "0 new finding(s)" for a text
        // that direct-curl matched at 0.9999.
        assert_eq!(
            applies_to_from_site("impl Wal for WalFile::prepare_wal_finish (line 3985)"),
            vec!["method"]
        );
    }

    #[test]
    fn applies_to_for_annotation_on_impl_block_itself_keeps_impl() {
        // No `::` in the site → the annotation is on the impl block
        // itself, not a method within. Keep `impl` so the server can
        // still filter against impl-targeted canon entries (if any).
        assert_eq!(
            applies_to_from_site("impl Wal for WalFile (line 3985)"),
            vec!["impl"]
        );
    }

    // ─── merge_response_into_cache: user-decision preservation ─────────────

    fn batch_entry(id: &str) -> BatchEntry {
        BatchEntry {
            id: aid(id),
            text: format!("text {id}"),
            text_hash: format!("sha256:{}", "a".repeat(64)),
            applies_to: vec!["fn".into()],
        }
    }

    fn canon_match_aristos() -> CanonMatch {
        CanonMatch {
            canon_id: "matched_canon".into(),
            version: "v0.1.0".into(),
            canonical_text: "matched text".into(),
            confidence: 0.92,
            scope: ":vanilla".into(),
            prefix_tier: PrefixTier::Aristos,
            backed_by: Some("specialized neural checker".into()),
            linked: Some("arta_xyz".into()),
            verification: VerificationMetadata {
                coverage_level: "tight".into(),
                test_binaries: vec![],
                instrumentation: None,
            },
        }
    }

    fn response_with(canon_matches_per_ann: Vec<Vec<CanonMatch>>) -> CanonMatchResponse {
        CanonMatchResponse {
            results: canon_matches_per_ann,
            effective_scopes: vec![":vanilla".into()],
            canon_version: "v0.2.0".into(),
            matched_at: "2026-06-15T09:14:22Z".into(),
            suggestions: None,
        }
    }

    #[test]
    fn merge_response_into_cache_writes_pending_for_each_match() {
        let mut cache = CanonMatchesFile::default();
        let batch = vec![batch_entry("alpha")];
        let response = response_with(vec![vec![canon_match_aristos()]]);

        let n = merge_response_into_cache(&mut cache, &batch, &response, "aristo stamp");
        assert_eq!(n, 1);
        let entry = &cache.entries[&aid("alpha")];
        assert_eq!(entry.pending_matches.len(), 1);
        assert_eq!(entry.pending_matches[0].canon_id, "matched_canon");
        assert_eq!(entry.pending_matches[0].found_by, "aristo stamp");
        assert!(matches!(
            entry.pending_matches[0].disposition,
            Disposition::Open
        ));
    }

    /// A small instrumentation bundle shaped like the golden fixture's
    /// record 1, for the P-008 carry tests below.
    fn sample_bundle() -> aristo_core::canon::InstrumentationBundle {
        use aristo_core::canon::{
            BundleCompanion, BundleCompileCheck, BundleProvenance, InstrumentationBundle,
            InstrumentationRecord, RecordLanding, RecordPresence,
        };
        let mut sut_binding = BTreeMap::new();
        sut_binding.insert("turso_core".to_string(), "core".to_string());
        InstrumentationBundle {
            bundle_id: "turso:7b6cbae:ae85f8792372".into(),
            provenance: BundleProvenance {
                base_ref: "ad351877c5cf38c1fafc7f08703bfe521b8f4437".into(),
                payload_ref: "7b6cbaec04e86c0d9ac47819c77444af5054c50a".into(),
                macro_grammar_rev: "aristo-macros 0.3.0 (two-mode Inspect grammar)".into(),
                sut_binding,
                authored_at: "7b6cbaec04e86c0d9ac47819c77444af5054c50a".into(),
            },
            compile_check: BundleCompileCheck {
                package: "turso_core".into(),
                features: "aristo-instr,turso_core/aristo-instr".into(),
            },
            companions: vec![BundleCompanion {
                symbol: "WalInstalledSnapshot".into(),
                role: "return_type".into(),
                file: "core/types.rs".into(),
                visibility: "pub (cfg aristo-instr)".into(),
                payload_ref: Some("7b6cbaec".into()),
            }],
            records: vec![InstrumentationRecord {
                accessor_id: "inspect_header_version".into(),
                kind: "inspect_projection".into(),
                class: "A".into(),
                semantic_tier: "none".into(),
                intent: "Expose the in-memory logical-log header version.".into(),
                catch: "Logical-log DOI catch (bug tag C-1).".into(),
                landing: RecordLanding {
                    target: serde_json::json!({
                        "crate": "turso_core",
                        "container": "LogicalLog",
                        "field": "header"
                    }),
                    annotation: Some(
                        "#[cfg_attr(feature = \"aristo-instr\", inspect(name = \"header_version\"))]"
                            .into(),
                    ),
                    ensure_derive: Some(
                        "#[cfg_attr(feature = \"aristo-instr\", derive(Inspect))]".into(),
                    ),
                    required_use: vec![],
                    companions_ref: vec!["WalInstalledSnapshot".into()],
                },
                presence: RecordPresence {
                    expected_symbol: "LogicalLog::inspect_header_version".into(),
                    expected_signature: "fn inspect_header_version(&self) -> Option<u8>".into(),
                    harness_probe: Some("let _r: Option<u8> = log.inspect_header_version();".into()),
                },
                oracle: None,
                upstream_status: "local-only".into(),
            }],
        }
    }

    #[test]
    fn merge_response_carries_verification_metadata_into_pending() {
        // P-008 carry (SLICE23-SPEC aristo item 2): the
        // match→PendingMatch builder used to DROP c.verification
        // entirely. It must now carry the whole block — coverage
        // level, routed test binaries, and the instrumentation
        // bundle — so `canon accept` can persist it.
        let mut cache = CanonMatchesFile::default();
        let batch = vec![batch_entry("alpha")];
        let mut m = canon_match_aristos();
        m.verification = aristo_core::canon::VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec!["wal_install_coherence".into()],
            instrumentation: Some(sample_bundle()),
        };
        let response = response_with(vec![vec![m]]);

        merge_response_into_cache(&mut cache, &batch, &response, "aristo stamp");

        let pending = &cache.entries[&aid("alpha")].pending_matches[0];
        let vm = pending
            .verification
            .as_ref()
            .expect("verification metadata must be carried, not dropped");
        assert_eq!(vm.coverage_level, "tight");
        assert_eq!(vm.test_binaries, vec!["wal_install_coherence"]);
        let bundle = vm
            .instrumentation
            .as_ref()
            .expect("instrumentation bundle must be carried");
        assert_eq!(bundle.bundle_id, "turso:7b6cbae:ae85f8792372");
        assert_eq!(bundle.records[0].accessor_id, "inspect_header_version");
        // A null-free bundle is carried verbatim (sanitizing is a no-op).
        assert_eq!(bundle, &sample_bundle());
    }

    #[test]
    fn merge_response_strips_bundle_value_nulls_so_cache_stays_toml_writable() {
        // The persistence hazard: `landing.target` is a verbatim
        // serde_json::Value, and TOML cannot represent null — an
        // unstripped null would make every subsequent cache write
        // fail at serialize time. The carry sanitizes before
        // persisting (dropping null-valued keys is decode-equivalent
        // to the wire's absent-vs-null rule).
        let mut cache = CanonMatchesFile::default();
        let batch = vec![batch_entry("alpha")];
        let mut m = canon_match_aristos();
        let mut bundle = sample_bundle();
        bundle.records[0].landing.target = serde_json::json!({
            "container": "LogicalLog",
            "field": "header",
            "stray_null": null
        });
        m.verification = aristo_core::canon::VerificationMetadata {
            coverage_level: "tight".into(),
            test_binaries: vec![],
            instrumentation: Some(bundle),
        };
        let response = response_with(vec![vec![m]]);

        merge_response_into_cache(&mut cache, &batch, &response, "aristo stamp");

        let carried = cache.entries[&aid("alpha")].pending_matches[0]
            .verification
            .as_ref()
            .unwrap()
            .instrumentation
            .as_ref()
            .unwrap();
        assert_eq!(
            carried.records[0].landing.target,
            serde_json::json!({ "container": "LogicalLog", "field": "header" }),
            "null-valued target keys must be stripped before persistence"
        );
        // The load-bearing property: the whole cache file serializes.
        let toml_text = toml::to_string_pretty(&cache)
            .expect("cache with a carried bundle must remain TOML-serializable");
        assert!(
            toml_text.contains("inspect_header_version"),
            "got: {toml_text}"
        );
    }

    #[test]
    fn merge_response_preserves_accepted_matches() {
        let mut cache = CanonMatchesFile::default();
        // Pre-populate alpha with an accepted match (user clicked accept).
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: format!("sha256:{}", "a".repeat(64)),
                canon_fetched_at: "2026-06-14T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![aristo_core::canon::AcceptedMatch {
                    canon_id: "previously_accepted".into(),
                    version: "v0.1.0".into(),
                    canonical_text: "previously accepted text".into(),
                    canon_version: "v0.2.0".into(),
                    confidence: 0.95,
                    prefix_tier: PrefixTier::Aristos,
                    backed_by: Some("specialized neural checker".into()),
                    linked: None,
                    verification: None,
                    accepted_at: "2026-06-14T00:00:00Z".into(),
                    bound_at: "2026-06-14T00:00:00Z".into(),
                }],
                rejected_matches: vec![],
            },
        );

        let batch = vec![batch_entry("alpha")];
        let response = response_with(vec![vec![canon_match_aristos()]]);
        merge_response_into_cache(&mut cache, &batch, &response, "aristo stamp");

        let entry = &cache.entries[&aid("alpha")];
        assert_eq!(entry.accepted_matches.len(), 1, "accepted must survive");
        assert_eq!(entry.accepted_matches[0].canon_id, "previously_accepted");
        assert_eq!(entry.pending_matches.len(), 1, "pending refreshed");
    }

    #[test]
    fn merge_response_suppresses_rejected_canon_id_for_same_text_hash() {
        let mut cache = CanonMatchesFile::default();
        let text_hash = format!("sha256:{}", "a".repeat(64));
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: text_hash.clone(),
                canon_fetched_at: "2026-06-14T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![],
                rejected_matches: vec![aristo_core::canon::RejectedMatch {
                    canon_id: "matched_canon".into(), // same canon_id as response will return
                    version: "v0.1.0".into(),
                    text_hash: text_hash.clone(),
                    rejected_at: "2026-06-14T01:00:00Z".into(),
                    reason: None,
                }],
            },
        );

        let batch = vec![batch_entry("alpha")];
        let response = response_with(vec![vec![canon_match_aristos()]]);
        let n = merge_response_into_cache(&mut cache, &batch, &response, "aristo stamp");

        // Rejected candidate suppressed — no pending added.
        assert_eq!(n, 0);
        let entry = &cache.entries[&aid("alpha")];
        assert!(entry.pending_matches.is_empty());
        // Rejection itself still in the cache.
        assert_eq!(entry.rejected_matches.len(), 1);
    }

    #[test]
    fn count_pending_sums_across_annotations() {
        let mut cache = CanonMatchesFile::default();
        cache.entries.insert(
            aid("alpha"),
            CacheEntry {
                last_match_text_hash: "x".into(),
                canon_fetched_at: "y".into(),
                pending_matches: vec![
                    PendingMatch {
                        canon_id: "c1".into(),
                        version: "v0.1.0".into(),
                        canonical_text: "x".into(),
                        canon_version: "v0.2.0".into(),
                        confidence: 0.9,
                        prefix_tier: PrefixTier::Aristos,
                        backed_by: None,
                        linked: Some("arta_x".into()),
                        verification: None,
                        disposition: Disposition::Open,
                        found_at: "t".into(),
                        found_by: "x".into(),
                    },
                    PendingMatch {
                        canon_id: "c2".into(),
                        version: "v0.1.0".into(),
                        canonical_text: "x".into(),
                        canon_version: "v0.2.0".into(),
                        confidence: 0.9,
                        prefix_tier: PrefixTier::Kanon,
                        backed_by: None,
                        linked: Some("arta_x".into()),
                        verification: None,
                        disposition: Disposition::Open,
                        found_at: "t".into(),
                        found_by: "x".into(),
                    },
                ],
                accepted_matches: vec![],
                rejected_matches: vec![],
            },
        );
        assert_eq!(count_pending(&cache), 2);
    }
}
