//! Canon-verify dispatch path for `aristo verify`.
//!
//! Replaces the original `pending_full > 0 => NotImplemented` arm in
//! `commands/verify/mod.rs`. For Full-resolved entries that are
//! canon-bound (`aristos:` or `kanon:`), this module:
//!
//! 1. Builds [`VerifySessionTag`]s from the index + canon-matches cache.
//! 2. Runs the push-first precheck via [`aristo_core::git`].
//! 3. POSTs `/canon/verify/sessions` and prints `session_id` +
//!    `view_url` for the user (detach default per WORKFLOW.md §7c row 1).
//!
//! Auth is required: the §14 design predicates canon-verify on having
//! an `arta_*` token (we re-derive scopes server-side and authorize
//! against `repository_flavors`). If no token is resolved, the SDK
//! surfaces an actionable "run `aristo auth login`" hint and skips —
//! it does NOT graceful-degrade (the user explicitly asked to verify;
//! we shouldn't silently no-op).
//!
//! Non-canon-bound `verify="full"` entries fall through to the
//! existing deferred-design `NotImplemented` arm — they're a separate
//! verification mechanism (see `docs/deferred/verify-test-design.md`)
//! and out of scope for §14.

use std::path::Path;

use aristo_core::canon::CanonMatchesFile;
use aristo_core::canon_verify::{
    HttpVerifyClient, PostVerifySessionResponse, VerifyClient, VerifyError, VerifySessionRequest,
    VerifySessionTag,
};
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, IntentEntry};

use crate::{CliError, CliResult};

/// Identifies an entry pending canon-verify dispatch. Lightweight —
/// the actual request body assembly happens in [`build_tags`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonDispatchEntry<'a> {
    pub id: &'a AnnotationId,
    pub entry: &'a IntentEntry,
}

/// From the full set of Full-resolved annotation ids (computed by the
/// existing dispatcher loop), partition them into the canon-verify
/// bucket vs. the legacy non-canon bucket.
///
/// Canon-bound = id namespace is `aristos:` or `kanon:` per
/// [`AnnotationId::is_canon_bound`].
pub(crate) fn partition_full<'a>(
    index: &'a IndexFile,
    pending_full_ids: &[&'a AnnotationId],
) -> (
    Vec<CanonDispatchEntry<'a>>,
    Vec<&'a AnnotationId>, // non-canon-bound; keep the NotImplemented hint
) {
    let mut canon: Vec<CanonDispatchEntry<'a>> = Vec::new();
    let mut other: Vec<&'a AnnotationId> = Vec::new();
    for id in pending_full_ids {
        let Some(entry) = index.entries.get(*id) else {
            continue;
        };
        let IndexEntry::Intent(intent) = entry else {
            // Assume entries don't carry `verify` (resolve to Bool(false));
            // they never reach the Full arm. Defensive.
            continue;
        };
        if id.is_canon_bound() {
            canon.push(CanonDispatchEntry {
                id,
                entry: intent,
            });
        } else {
            other.push(id);
        }
    }
    (canon, other)
}

/// Build the [`VerifySessionTag`] list for a set of canon-bound
/// Full-resolved entries. Joins the index entry (for `annotation_id`
/// via `binding.linked` + source_path via `file:line`) with the
/// canon-matches cache (for `canon_id` + `version`).
///
/// Entries whose `linked` is missing (Local binding state — which
/// shouldn't happen for canon-prefixed ids but is defensively
/// handled), whose canon-matches entry is absent, or whose accepted-
/// match list is empty are silently dropped from the dispatch.
/// They'd fail the server-side eligibility check anyway and surface
/// a clearer error on the next `aristo canon refresh`.
pub(crate) fn build_tags(
    entries: &[CanonDispatchEntry<'_>],
    matches: &CanonMatchesFile,
) -> Vec<VerifySessionTag> {
    entries
        .iter()
        .filter_map(|d| build_one_tag(d, matches))
        .collect()
}

fn build_one_tag(
    dispatch: &CanonDispatchEntry<'_>,
    matches: &CanonMatchesFile,
) -> Option<VerifySessionTag> {
    let annotation_id = match &dispatch.entry.binding {
        aristo_core::index::BindingState::Local => return None,
        aristo_core::index::BindingState::Bound { linked } => linked.as_str().to_string(),
        aristo_core::index::BindingState::Certified { linked, .. } => {
            linked.as_str().to_string()
        }
    };
    // Strip `aristos:` / `kanon:` prefix to get the bare canon_id.
    let canon_id = strip_canon_prefix(dispatch.id.as_str()).to_string();
    // Pull version from canon-matches cache. The cache is keyed by
    // the FULL AnnotationId (matching the source-form id used in
    // both source and index).
    let cache_entry = matches.entries.get(dispatch.id)?;
    let accepted = cache_entry.accepted_matches.first()?;
    let version = accepted.version.clone();

    let source_path = format_source_path(&dispatch.entry.file, dispatch.entry.site.as_str());

    Some(VerifySessionTag {
        annotation_id,
        canon_id,
        version,
        source_path,
    })
}

fn strip_canon_prefix(id: &str) -> &str {
    id.strip_prefix("aristos:")
        .or_else(|| id.strip_prefix("kanon:"))
        .unwrap_or(id)
}

/// Format `file:line` from the index entry's `file` + the line
/// extracted from `site`. The site format is `"fn foo (line N)"`
/// per the walker / index emit; absent a line (defensive), we emit
/// just the file path so the user still sees a clickable hint.
fn format_source_path(file: &str, site: &str) -> String {
    match extract_line(site) {
        Some(line) => format!("{file}:{line}"),
        None => file.to_string(),
    }
}

fn extract_line(site: &str) -> Option<u32> {
    let after = site.split(" (line ").nth(1)?;
    let trimmed = after.trim_end_matches(')');
    trimmed.parse().ok()
}

/// What the dispatcher does after partitioning + tag-building. Surfaced
/// as a separate fn so tests can drive it with a mock client.
#[cfg(test)]
pub(crate) fn dispatch_session<C: VerifyClient + ?Sized>(
    client: &C,
    req: &VerifySessionRequest,
) -> Result<PostVerifySessionResponse, VerifyError> {
    client.post_session(req)
}

/// End-to-end canon-verify dispatch:
///
/// 1. Build the tag list (silent skip for entries with no
///    canon-matches cache entry — they'd 4xx anyway).
/// 2. If empty, return 0 (nothing to dispatch — the existing skip-
///    counters in `emit_summary` already informed the user).
/// 3. Push-first precheck via git.
/// 4. Derive `commit_sha` + `repo_full_name`.
/// 5. Construct the HTTP client + POST.
/// 6. Print `session_id` + `view_url`.
///
/// Returns the number of tags actually dispatched (for the summary
/// emit on the caller side). Errors are CLI errors so the caller can
/// `?` them up.
pub(crate) fn run_canon_dispatch(
    workspace_root: &Path,
    matches_path: &Path,
    canon_entries: &[CanonDispatchEntry<'_>],
) -> CliResult<usize> {
    if canon_entries.is_empty() {
        return Ok(0);
    }

    // 1. Auth — required up front. If no token, surface actionable hint.
    let creds = aristo_core::auth::resolve_full().map_err(no_auth_to_cli_error)?;

    // 2. Build tags from index + cache. Drop the no-version cases.
    let matches = CanonMatchesFile::read(matches_path).map_err(|e| CliError::Other {
        message: format!(
            "failed to read canon-matches cache at {}: {e}",
            matches_path.display()
        ),
        exit_code: 1,
    })?;
    let tags = build_tags(canon_entries, &matches);
    if tags.is_empty() {
        // Every canon-bound entry was missing a cache entry. Surface
        // a helpful message rather than POSTing empty.
        println!(
            "\n→ {} canon-bound entries are pending verification, but no `accepted_matches` were found in",
            canon_entries.len()
        );
        println!(
            "  .aristo/canon-matches.toml. Run `aristo canon refresh` to repopulate the cache."
        );
        return Ok(0);
    }

    // 3. Resolve repo + commit_sha via git.
    let repo_full_name = match aristo_core::auth::derive_repo_full_name(workspace_root) {
        Ok(r) => r,
        Err(e) => {
            // ARISTO_REPO env override as a CI escape hatch.
            std::env::var("ARISTO_REPO").map_err(|_| CliError::Other {
                message: format!(
                    "could not determine repo for canon-verify: {e}\n  \
                     Set ARISTO_REPO=<owner/repo> to override (CI use)."
                ),
                exit_code: 1,
            })?
        }
    };
    let commit_sha = aristo_core::git::rev_parse_head(workspace_root).map_err(|e| {
        CliError::Other {
            message: format!("git rev-parse HEAD failed: {e}"),
            exit_code: 1,
        }
    })?;

    // 4. Push-first precheck (WORKFLOW.md §4 + §7c).
    let pushed = aristo_core::git::commit_present_on_remote(workspace_root, &commit_sha)
        .map_err(|e| CliError::Other {
            message: format!("git push-first precheck failed: {e}"),
            exit_code: 1,
        })?;
    if !pushed {
        return Err(CliError::Other {
            message: format!(
                "HEAD ({}) is not pushed to origin. Push your branch first; \
                 `aristo verify --watch` for local-edit sync is planned but \
                 not yet shipped.",
                short_sha(&commit_sha)
            ),
            exit_code: 1,
        });
    }

    // 5. Build the HTTP client. ARETTA_API_URL overrides for tests.
    let base_url = std::env::var("ARETTA_API_URL")
        .unwrap_or_else(|_| creds.server.as_str().to_string());
    let client: Box<dyn VerifyClient> =
        if let Some(mock) = test_mock_client_from_env() {
            mock
        } else {
            Box::new(HttpVerifyClient::new(base_url, &creds.token))
        };

    // 6. POST.
    let req = VerifySessionRequest {
        repo_full_name,
        commit_sha,
        tags,
    };
    let resp = client.post_session(&req).map_err(verify_error_to_cli)?;

    // 7. Print user-facing summary (detach default per §7c row 1).
    let dispatched = req.tags.len();
    print_session_dispatched(&req, &resp);
    Ok(dispatched)
}

fn print_session_dispatched(req: &VerifySessionRequest, resp: &PostVerifySessionResponse) {
    let annotation_word = if resp.plan_size == 1 {
        "annotation"
    } else {
        "annotations"
    };
    println!();
    println!(
        "→ canon-verify session dispatched — verifying {} {annotation_word} against {}",
        resp.plan_size,
        short_sha(&req.commit_sha)
    );
    println!("  session: {}", resp.session_id);
    println!("  view:    {}", resp.view_url);
    println!();
    println!(
        "  Re-attach with: aristo verify --view {} --wait",
        resp.session_id
    );
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn no_auth_to_cli_error(e: aristo_core::auth::AuthError) -> CliError {
    CliError::Other {
        message: format!(
            "canon-verify requires authentication: {e}\n  \
             Run `aristo auth login` to sign in.\n  \
             (Without a token, the SDK skips canon-bound `verify=\"full\"` \
             entries — they'd be rejected server-side anyway.)"
        ),
        exit_code: 1,
    }
}

fn verify_error_to_cli(e: VerifyError) -> CliError {
    match e {
        VerifyError::Auth(inner) => CliError::Other {
            message: format!(
                "canon-verify auth error: {inner}\n  \
                 Your token may be expired — re-run `aristo auth login`."
            ),
            exit_code: 1,
        },
        VerifyError::BadRequest { status, message } if status == 402 => CliError::Other {
            message: format!(
                "no canon coverage applies for your scopes — \
                 contact Aretta for DP onboarding to enable verification.\n  \
                 (server message: {message})"
            ),
            exit_code: 1,
        },
        VerifyError::BadRequest { status, message } => CliError::Other {
            message: format!("canon-verify server rejected request (HTTP {status}): {message}"),
            exit_code: 1,
        },
        other => CliError::Other {
            message: format!("canon-verify failed: {other}"),
            exit_code: 1,
        },
    }
}

// ─── Test-only mock plumbing ────────────────────────────────────────────────

/// Build a [`VerifyClient`] from `ARISTO_CANON_VERIFY_FIXTURE` if set.
///
/// The env var points at a JSON file with the canned POST response.
/// Used by CLI integration tests to exercise the dispatch path
/// without spinning up an HTTP server. Production callers must never
/// set this; the file-on-disk gating + env var make accidental
/// production use loud.
fn test_mock_client_from_env() -> Option<Box<dyn VerifyClient>> {
    let path = std::env::var("ARISTO_CANON_VERIFY_FIXTURE").ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let parsed: FixtureFile = serde_json::from_str(&raw).ok()?;
    let post_resp = parsed.post.map(|p| PostVerifySessionResponse {
        session_id: p.session_id,
        view_url: p.view_url,
        plan_size: p.plan_size,
    })?;
    // Record the POST body to a sibling file so tests can inspect it.
    let record_path = format!("{path}.posted.json");
    let mock = aristo_core::canon_verify::MockVerifyClient::with_post_response(post_resp);
    Some(Box::new(RecordingMock {
        inner: mock,
        record_path,
    }))
}

#[derive(serde::Deserialize)]
struct FixtureFile {
    post: Option<FixturePost>,
}

#[derive(serde::Deserialize)]
struct FixturePost {
    session_id: String,
    view_url: String,
    plan_size: u32,
}

/// Wraps [`MockVerifyClient`] to write the POSTed request body to a
/// sidecar file so the CLI integration test can assert wire-shape.
struct RecordingMock {
    inner: aristo_core::canon_verify::MockVerifyClient,
    record_path: String,
}

impl VerifyClient for RecordingMock {
    fn post_session(
        &self,
        req: &VerifySessionRequest,
    ) -> Result<PostVerifySessionResponse, VerifyError> {
        if let Ok(serialized) = serde_json::to_string_pretty(req) {
            let _ = std::fs::write(&self.record_path, serialized);
        }
        self.inner.post_session(req)
    }

    fn get_session(
        &self,
        session_id: &str,
        wait_seconds: Option<u32>,
    ) -> Result<aristo_core::canon_verify::GetVerifySessionResponse, VerifyError> {
        self.inner.get_session(session_id, wait_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::canon::{AcceptedMatch, CacheEntry, CacheMeta, PrefixTier};
    use aristo_core::index::{
        ArtaId, BindingState, CoveredRegion, IndexEntry, IntentEntry, Meta, Sha256, Status,
        VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

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

    fn zero_hash() -> Sha256 {
        Sha256::parse(&format!("sha256:{}", "0".repeat(64))).unwrap()
    }

    fn arta(s: &str) -> ArtaId {
        ArtaId::parse(s).unwrap()
    }

    fn intent(
        id: &str,
        binding: BindingState,
        file: &str,
        site_with_line: &str,
    ) -> (AnnotationId, IntentEntry) {
        (
            AnnotationId::parse(id).unwrap(),
            IntentEntry {
                text: "the property".into(),
                verify: VerifyLevel::Method(VerifyMethod::Full),
                status: Status::Unknown,
                text_hash: zero_hash(),
                body_hash: zero_hash(),
                file: file.into(),
                site: site_with_line.into(),
                covered_region: CoveredRegion::Function,
                binding,
                parent: None,
                last_critiqued_at_text_hash: None,
                last_critique_finding_count: None,
            },
        )
    }

    fn matches_with(id: &AnnotationId, canon_id: &str, version: &str) -> CanonMatchesFile {
        let mut f = CanonMatchesFile::default();
        f.meta = CacheMeta::default();
        f.entries.insert(
            id.clone(),
            CacheEntry {
                last_match_text_hash: "x".into(),
                canon_fetched_at: "2026-05-24T00:00:00Z".into(),
                pending_matches: vec![],
                accepted_matches: vec![AcceptedMatch {
                    canon_id: canon_id.into(),
                    version: version.into(),
                    canonical_text: "the property".into(),
                    canon_version: "v0.2.0".into(),
                    confidence: 0.95,
                    prefix_tier: PrefixTier::Aristos,
                    backed_by: Some("test backing".into()),
                    accepted_at: "2026-05-24T00:00:00Z".into(),
                    bound_at: "2026-05-24T00:00:00Z".into(),
                }],
                rejected_matches: vec![],
            },
        );
        f
    }

    // ─── partition_full ───────────────────────────────────────────────────

    #[test]
    fn partition_separates_canon_bound_from_local() {
        let (aristos_id, aristos_entry) = intent(
            "aristos:foo",
            BindingState::Bound {
                linked: arta("arta_op4q3z9NbV"),
            },
            "src/x.rs",
            "fn x (line 1)",
        );
        let (kanon_id, kanon_entry) = intent(
            "kanon:bar",
            BindingState::Bound {
                linked: arta("arta_xyz1234567"),
            },
            "src/y.rs",
            "fn y (line 10)",
        );
        let (local_id, local_entry) =
            intent("baz", BindingState::Local, "src/z.rs", "fn z (line 5)");

        let mut index = empty_index();
        index
            .entries
            .insert(aristos_id.clone(), IndexEntry::Intent(aristos_entry));
        index
            .entries
            .insert(kanon_id.clone(), IndexEntry::Intent(kanon_entry));
        index
            .entries
            .insert(local_id.clone(), IndexEntry::Intent(local_entry));

        let pending = vec![&aristos_id, &kanon_id, &local_id];
        let (canon, other) = partition_full(&index, &pending);

        assert_eq!(canon.len(), 2);
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].as_str(), "baz");
        let canon_ids: Vec<_> = canon.iter().map(|c| c.id.as_str()).collect();
        assert!(canon_ids.contains(&"aristos:foo"));
        assert!(canon_ids.contains(&"kanon:bar"));
    }

    #[test]
    fn partition_returns_empty_for_no_input() {
        let index = empty_index();
        let (canon, other) = partition_full(&index, &[]);
        assert!(canon.is_empty());
        assert!(other.is_empty());
    }

    // ─── build_tags ───────────────────────────────────────────────────────

    #[test]
    fn build_tags_emits_arta_id_canon_id_version_and_source_path() {
        let (id, entry) = intent(
            "aristos:vacuum_preserves_logical_content",
            BindingState::Bound {
                linked: arta("arta_op4q3z9NbV"),
            },
            "crates/vacuum/src/lib.rs",
            "fn vacuum (line 42)",
        );
        let matches = matches_with(&id, "vacuum_preserves_logical_content", "v0.1.0");
        let dispatch = vec![CanonDispatchEntry { id: &id, entry: &entry }];

        let tags = build_tags(&dispatch, &matches);
        assert_eq!(tags.len(), 1);
        let t = &tags[0];
        assert_eq!(t.annotation_id, "arta_op4q3z9NbV");
        assert_eq!(t.canon_id, "vacuum_preserves_logical_content");
        assert_eq!(t.version, "v0.1.0");
        assert_eq!(t.source_path, "crates/vacuum/src/lib.rs:42");
    }

    #[test]
    fn build_tags_works_for_certified_binding_state() {
        // Certified binding still carries `linked` — the SDK should
        // re-dispatch on --rerun (caller's responsibility; we just
        // need to handle the variant).
        use aristo_core::index::{CommitHash, VerifiedOutcome};
        let (id, entry) = intent(
            "kanon:foo",
            BindingState::Certified {
                linked: arta("arta_aaaa1234"),
                verified_outcome: VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86)))
                    .unwrap(),
                last_verified_at_commit: CommitHash::parse(&"a".repeat(40)).unwrap(),
            },
            "src/foo.rs",
            "fn foo (line 7)",
        );
        let matches = matches_with(&id, "foo", "v0.2.0");
        let dispatch = vec![CanonDispatchEntry { id: &id, entry: &entry }];
        let tags = build_tags(&dispatch, &matches);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].annotation_id, "arta_aaaa1234");
        assert_eq!(tags[0].canon_id, "foo");
        assert_eq!(tags[0].version, "v0.2.0");
    }

    #[test]
    fn build_tags_drops_entries_missing_from_cache() {
        // Canon-bound entry but no cache row — the cache is the only
        // source of truth for `version`. Drop silently rather than
        // POST an empty version that'd 4xx server-side.
        let (id, entry) = intent(
            "aristos:foo",
            BindingState::Bound {
                linked: arta("arta_aaaa1234"),
            },
            "src/foo.rs",
            "fn foo (line 7)",
        );
        let matches = CanonMatchesFile::default();
        let dispatch = vec![CanonDispatchEntry { id: &id, entry: &entry }];
        let tags = build_tags(&dispatch, &matches);
        assert!(tags.is_empty(), "missing cache entry must drop the tag");
    }

    #[test]
    fn build_tags_drops_local_binding() {
        // Defensive: canon-prefixed id but `BindingState::Local` —
        // shouldn't happen in practice but the partition is by id-
        // prefix only; the build must handle missing linked gracefully.
        let (id, entry) = intent(
            "aristos:foo",
            BindingState::Local,
            "src/foo.rs",
            "fn foo (line 7)",
        );
        let matches = matches_with(&id, "foo", "v0.1.0");
        let dispatch = vec![CanonDispatchEntry { id: &id, entry: &entry }];
        let tags = build_tags(&dispatch, &matches);
        assert!(tags.is_empty());
    }

    #[test]
    fn build_tags_falls_back_to_file_only_when_site_lacks_line() {
        let (id, entry) = intent(
            "aristos:foo",
            BindingState::Bound {
                linked: arta("arta_aaaa1234"),
            },
            "src/foo.rs",
            "fn foo", // no "(line N)" suffix
        );
        let matches = matches_with(&id, "foo", "v0.1.0");
        let dispatch = vec![CanonDispatchEntry { id: &id, entry: &entry }];
        let tags = build_tags(&dispatch, &matches);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].source_path, "src/foo.rs");
    }

    // ─── strip_canon_prefix ──────────────────────────────────────────────

    #[test]
    fn strip_canon_prefix_handles_both_tiers_and_no_prefix() {
        assert_eq!(strip_canon_prefix("aristos:foo_bar"), "foo_bar");
        assert_eq!(strip_canon_prefix("kanon:checkout_total_non_negative"), "checkout_total_non_negative");
        // No prefix — pass through (defensive).
        assert_eq!(strip_canon_prefix("local_id"), "local_id");
    }

    // ─── extract_line ────────────────────────────────────────────────────

    #[test]
    fn extract_line_parses_walker_emitted_site_format() {
        assert_eq!(extract_line("fn foo (line 42)"), Some(42));
        assert_eq!(extract_line("fn really::long::path (line 1)"), Some(1));
        assert_eq!(extract_line("fn x"), None);
        assert_eq!(extract_line(""), None);
    }

    // ─── dispatch_session (with mock client) ─────────────────────────────

    #[test]
    fn dispatch_session_round_trips_through_mock() {
        let mock = aristo_core::canon_verify::MockVerifyClient::with_post_response(
            PostVerifySessionResponse {
                session_id: "01HMTEST".into(),
                view_url: "https://dev.aretta.ai/dashboard/jobs/01HMTEST".into(),
                plan_size: 1,
            },
        );
        let req = VerifySessionRequest {
            repo_full_name: "owner/repo".into(),
            commit_sha: "deadbeef".into(),
            tags: vec![VerifySessionTag {
                annotation_id: "arta_x".into(),
                canon_id: "foo".into(),
                version: "v0.1.0".into(),
                source_path: "src/x.rs:1".into(),
            }],
        };
        let resp = dispatch_session(&mock, &req).expect("dispatch ok");
        assert_eq!(resp.session_id, "01HMTEST");
        let posted = mock.posted_requests();
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].tags[0].annotation_id, "arta_x");
    }

    #[test]
    fn dispatch_session_propagates_server_error() {
        let mock = aristo_core::canon_verify::MockVerifyClient::with_post_error(
            VerifyError::BadRequest {
                status: 402,
                message: "no_canon_coverage".into(),
            },
        );
        let req = VerifySessionRequest {
            repo_full_name: "o/r".into(),
            commit_sha: "x".into(),
            tags: vec![],
        };
        let err = dispatch_session(&mock, &req).unwrap_err();
        match err {
            VerifyError::BadRequest { status: 402, .. } => {}
            other => panic!("expected BadRequest 402, got {other:?}"),
        }
    }

}
