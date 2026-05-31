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

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use aristo_core::canon::CanonMatchesFile;
use aristo_core::canon_verify::{
    AnnotationOutcomeStatus, DifferentialReport, Finding, GetVerifySessionResponse,
    HttpVerifyClient, PostVerifySessionResponse, SessionStatus, TestOutcome, TestOutcomeStatus,
    VerifyClient, VerifyError, VerifySessionRequest, VerifySessionTag,
};
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile, IntentEntry};

use crate::{CliError, CliResult};

/// Long-poll server-side hold-time (§7c row 9). The server may ignore
/// this and return immediately in the prototype — the SDK still sends
/// the hint for forward-compat.
const LONGPOLL_WAIT_SECS: u32 = 30;

/// Default between-poll backoff when the server returns immediately.
/// Keeps the CLI rendering smooth (§7c row 9 "3s render") and bounded.
/// Overridable via `ARISTO_VERIFY_POLL_MS` for tests; production
/// callers never set it.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

fn poll_interval() -> Duration {
    if let Ok(ms) = std::env::var("ARISTO_VERIFY_POLL_MS") {
        if let Ok(n) = ms.parse::<u64>() {
            return Duration::from_millis(n);
        }
    }
    POLL_INTERVAL
}

/// Heartbeat cadence (§7c row 13: "still running… (N seconds elapsed)").
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

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
            canon.push(CanonDispatchEntry { id, entry: intent });
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
        aristo_core::index::BindingState::Certified { linked, .. } => linked.as_str().to_string(),
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
    tags_filter: Option<&[String]>,
    wait: bool,
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
    let mut tags = build_tags(canon_entries, &matches);

    // 2b. Optional --tags filter (E4): subset to the requested ids.
    // Validates each requested id (reject `arta_*` — those are server-
    // side opaque refs that the user never sees in source).
    if let Some(requested) = tags_filter {
        let allowed = build_tag_filter_set(canon_entries, requested)?;
        tags.retain(|t| allowed.contains(&t.annotation_id));
        if tags.is_empty() {
            return Err(CliError::Other {
                message: format!(
                    "--tags filter matched zero eligible canon-bound entries. \
                     Requested ids: {}",
                    requested.join(", ")
                ),
                exit_code: 1,
            });
        }
    }
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
                    "could not determine repo for verify: {e}\n  \
                     Set ARISTO_REPO=<owner/repo> to override (CI use)."
                ),
                exit_code: 1,
            })?
        }
    };
    let commit_sha =
        aristo_core::git::rev_parse_head(workspace_root).map_err(|e| CliError::Other {
            message: format!("git rev-parse HEAD failed: {e}"),
            exit_code: 1,
        })?;

    // 4. Push-first precheck (WORKFLOW.md §4 + §7c).
    let pushed =
        aristo_core::git::commit_present_on_remote(workspace_root, &commit_sha).map_err(|e| {
            CliError::Other {
                message: format!("git push-first precheck failed: {e}"),
                exit_code: 1,
            }
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
    let base_url =
        std::env::var("ARETTA_API_URL").unwrap_or_else(|_| creds.server.as_str().to_string());
    let client: Box<dyn VerifyClient> = if let Some(mock) = test_mock_client_from_env() {
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
    let dispatched = req.tags.len();
    print_session_dispatched(&req, &resp);

    // 7. Optional poll-to-completion (--wait).
    if wait {
        let final_snapshot = poll_until_terminal(&*client, &resp.session_id)?;
        render_final_snapshot(&final_snapshot);
        if !final_snapshot.summary.is_success() {
            // §7c row 7: any failed / build_failed / inconclusive → exit 1.
            return Err(CliError::Other {
                message: format!(
                    "verify reported {} failed, {} build_failed, {} inconclusive",
                    final_snapshot.summary.failed,
                    final_snapshot.summary.build_failed,
                    final_snapshot.summary.inconclusive
                ),
                exit_code: 1,
            });
        }
    }

    Ok(dispatched)
}

/// Re-attach to an existing session (E3 — `aristo verify --view <id>`).
/// One GET (or polling loop with --wait), render, exit-code derive.
/// Skips POST + push-first precheck entirely.
pub(crate) fn run_view_session(session_id: &str, wait: bool) -> CliResult<()> {
    let creds = aristo_core::auth::resolve_full().map_err(no_auth_to_cli_error)?;
    let base_url =
        std::env::var("ARETTA_API_URL").unwrap_or_else(|_| creds.server.as_str().to_string());
    let client: Box<dyn VerifyClient> = if let Some(mock) = test_mock_client_from_env() {
        mock
    } else {
        Box::new(HttpVerifyClient::new(base_url, &creds.token))
    };

    let snapshot = if wait {
        poll_until_terminal(&*client, session_id)?
    } else {
        client
            .get_session(session_id, None)
            .map_err(verify_error_to_cli)?
    };
    render_final_snapshot(&snapshot);
    if wait && !snapshot.summary.is_success() {
        return Err(CliError::Other {
            message: format!(
                "verify reported {} failed, {} build_failed, {} inconclusive",
                snapshot.summary.failed,
                snapshot.summary.build_failed,
                snapshot.summary.inconclusive
            ),
            exit_code: 1,
        });
    }
    Ok(())
}

/// Long-poll loop until the session reaches a terminal state. Renders
/// an intermediate snapshot at first non-terminal response so the user
/// sees the in-flight state; emits a heartbeat every 60s.
fn poll_until_terminal<C: VerifyClient + ?Sized>(
    client: &C,
    session_id: &str,
) -> CliResult<GetVerifySessionResponse> {
    let started = Instant::now();
    let mut last_heartbeat = started;
    let mut intermediate_rendered = false;

    loop {
        let snapshot = client
            .get_session(session_id, Some(LONGPOLL_WAIT_SECS))
            .map_err(verify_error_to_cli)?;
        if snapshot.status.is_terminal() {
            return Ok(snapshot);
        }
        if !intermediate_rendered {
            // First non-terminal poll: show what's running.
            println!();
            println!(
                "  status: {} ({} of {} annotations verified so far)",
                status_label(snapshot.status),
                snapshot.summary.verified,
                snapshot.summary.total_annotations
            );
            intermediate_rendered = true;
        }
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            let elapsed = started.elapsed().as_secs();
            println!("  still running… ({elapsed}s elapsed)");
            last_heartbeat = Instant::now();
        }
        // Back off briefly so we don't hammer when the server returns
        // immediately (i.e., when ?wait= is ignored — current proxy
        // state).
        std::thread::sleep(poll_interval());
    }
}

fn build_tag_filter_set(
    canon_entries: &[CanonDispatchEntry<'_>],
    requested: &[String],
) -> CliResult<HashSet<String>> {
    let mut allowed: HashSet<String> = HashSet::new();
    // Build a map from canon-bound AnnotationId.as_str() → linked arta_*
    // so the user can pass either source-form ids (`aristos:foo`) or
    // canon-id suffixes (`foo`) without surprises.
    let mut by_source: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let mut by_canon: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for d in canon_entries {
        let linked = match &d.entry.binding {
            aristo_core::index::BindingState::Bound { linked } => linked.as_str().to_string(),
            aristo_core::index::BindingState::Certified { linked, .. } => {
                linked.as_str().to_string()
            }
            aristo_core::index::BindingState::Local => continue,
        };
        by_source.insert(d.id.as_str(), linked.clone());
        let canon = strip_canon_prefix(d.id.as_str());
        by_canon.insert(canon, linked);
    }
    for raw in requested {
        let id = raw.trim();
        if id.is_empty() {
            continue;
        }
        if id.starts_with("arta_") {
            return Err(CliError::Other {
                message: format!(
                    "--tags rejects opaque server ids (got `{id}`). Pass the source-form id \
                     instead, e.g. `--tags aristos:foo,kanon:bar`."
                ),
                exit_code: 2,
            });
        }
        if let Some(linked) = by_source.get(id) {
            allowed.insert(linked.clone());
        } else if let Some(linked) = by_canon.get(id) {
            allowed.insert(linked.clone());
        } else {
            return Err(CliError::Other {
                message: format!(
                    "--tags id `{id}` is not a canon-bound entry in this workspace's index"
                ),
                exit_code: 1,
            });
        }
    }
    Ok(allowed)
}

// ─── Rendering (WORKFLOW.md §6 — CLI form) ──────────────────────────────────

fn render_final_snapshot(snapshot: &GetVerifySessionResponse) {
    println!();
    println!(
        "session {} — verifying {} against canon {}",
        snapshot.session_id,
        short_sha(&snapshot.user_commit_sha),
        snapshot.canon_version
    );
    println!(
        "status: {} ({}/{} verified)",
        status_label(snapshot.status),
        snapshot.summary.verified,
        snapshot.summary.total_annotations
    );
    println!();
    println!("{:<55}  {:<14} TESTS", "ANNOTATION", "STATUS");
    for ann in &snapshot.annotations {
        let icon = annotation_icon(ann.status);
        let passed = ann
            .tests
            .iter()
            .filter(|t| matches!(t.status, TestOutcomeStatus::Pass))
            .count();
        let header = format!(
            "{}{}@{} ({})",
            ann.tier, ann.canon_id, ann.version, ann.scope
        );
        let tests_summary = if ann.tests.is_empty() {
            "(no coverage)".to_string()
        } else {
            format!("{}/{} passed", passed, ann.tests.len())
        };
        println!(
            "{:<55}  {} {:<11}  {}",
            header,
            icon,
            annotation_status_word(ann.status),
            tests_summary
        );
        println!("  {}", ann.source_path);
        for t in &ann.tests {
            if !matches!(
                t.status,
                TestOutcomeStatus::Fail
                    | TestOutcomeStatus::BuildFailed
                    | TestOutcomeStatus::CloneFailed
                    | TestOutcomeStatus::Timeout
                    | TestOutcomeStatus::Error
            ) {
                continue;
            }
            // Phase 16: a structured DifferentialReport renders a
            // violation card instead of the terse bullet. Fall back to
            // the bullet when no report is attached.
            match &t.report {
                Some(report) => render_report_card(report),
                None => render_test_bullet(t),
            }
        }
    }
    println!();
    // No `view_url` on GetVerifySessionResponse; the dashboard link is
    // derivable from session_id when needed but we surface only the
    // session id here to keep the output tight.
}

/// The terse fall-back row for a failing test with no attached report:
/// `• <bin> <word> — see <stderr_url>`.
fn render_test_bullet(t: &TestOutcome) {
    let bin = t.test_binary.as_deref().unwrap_or("(session)");
    let word = test_status_word(t.status);
    match &t.stderr_url {
        Some(url) => println!("  • {bin} {word} — see {url}"),
        None => println!("  • {bin} {word}"),
    }
}

/// Phase 16 Track A — render a [`DifferentialReport`] as a structured
/// violation card (Slice-1 shape). Hand-emitted plain ASCII; the CLI
/// has no color/table/box crate and that's fine. Every value is driven
/// off the report fields; nothing is hard-coded.
fn render_report_card(report: &DifferentialReport) {
    let Finding::StateEq {
        expected,
        actual,
        divergence,
    } = &report.finding;

    println!();
    // Headline + plain-language statement.
    println!("  ✗ PROPERTY VIOLATED   {}", report.property.canon_id);
    println!("    {}", report.property.statement);
    println!();

    // spec / impl source anchors (each optional).
    if report.property.spec_source.is_some() || report.property.impl_source.is_some() {
        let spec = report
            .property
            .spec_source
            .as_ref()
            .map(|s| format!("spec  {}:{}", s.path, s.line))
            .unwrap_or_default();
        let imp = report.property.impl_source.as_ref().map(|s| {
            let loc = format!("impl  {}:{}", s.path, s.line);
            match &s.snippet {
                Some(snip) => format!("{loc} ({snip})"),
                None => loc,
            }
        });
        match imp {
            Some(imp) if !spec.is_empty() => println!("    {spec}            {imp}"),
            Some(imp) => println!("    {imp}"),
            None => println!("    {spec}"),
        }
        println!();
    }

    // The mask: compared fields + how many were ignored.
    let compared = report.relation.compared.join(", ");
    let ignored = render_ignored(&report.relation.ignored);
    println!("    Compared: [{compared}]   (ignored: {ignored})");
    println!();

    // Two-column snapshot labels, then the -/+ divergence rows.
    println!("        {}      {}", expected.label, actual.label);
    for d in divergence {
        println!("      - {} = {}", d.field, d.expected);
        println!("      + {} = {}", d.field, d.actual);
        if let Some(why) = &d.provenance {
            println!("        why  {why}");
        }
    }
    println!();

    // Verdict frame.
    if let Some(cr_id) = &report.verdict.cr_id {
        match &report.verdict.expected_to_fail {
            Some(etf) => {
                println!(
                    "    Verdict   {cr_id} · EXPECTED TO FAIL (the failure IS the conformance verdict)"
                );
                println!("    Unblocks  {}", etf.reason);
            }
            None => println!("    Verdict   {cr_id}"),
        }
    } else if let Some(etf) = &report.verdict.expected_to_fail {
        println!("    Verdict   EXPECTED TO FAIL (the failure IS the conformance verdict)");
        println!("    Unblocks  {}", etf.reason);
    }
    println!();

    // Reproduce hint, derived from canon_id + seed.
    println!("    Reproduce");
    println!(
        "      aristo verify --case {} --replay",
        reproduce_case(report)
    );
    println!();
}

/// Render the ignored-mask suffix: first two names, then `+N` for the
/// remainder, e.g. `max_frame, max_frame_inflight, +7`.
fn render_ignored(ignored: &[String]) -> String {
    if ignored.is_empty() {
        return "none".to_string();
    }
    let head: Vec<&str> = ignored.iter().take(2).map(String::as_str).collect();
    let rest = ignored.len().saturating_sub(head.len());
    if rest > 0 {
        format!("{}, +{rest}", head.join(", "))
    } else {
        head.join(", ")
    }
}

/// Derive the `--case` repro token. Prefer the scenario seed (the
/// deterministic repro key); fall back to the canon id.
fn reproduce_case(report: &DifferentialReport) -> &str {
    let seed = report.scenario.seed.as_str();
    if seed.is_empty() {
        report.property.canon_id.as_str()
    } else {
        seed
    }
}

fn status_label(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Queued => "queued",
        SessionStatus::Running => "running",
        SessionStatus::Done => "done",
        SessionStatus::Failed => "failed",
        SessionStatus::TimedOut => "timed out",
        SessionStatus::Cancelled => "cancelled",
    }
}

fn annotation_icon(s: AnnotationOutcomeStatus) -> &'static str {
    match s {
        AnnotationOutcomeStatus::Verified => "[ok]",
        AnnotationOutcomeStatus::Failed => "[fail]",
        AnnotationOutcomeStatus::BuildFailed => "[warn]",
        AnnotationOutcomeStatus::Inconclusive => "[?]",
        AnnotationOutcomeStatus::NoCoverage => "[--]",
    }
}

fn annotation_status_word(s: AnnotationOutcomeStatus) -> &'static str {
    match s {
        AnnotationOutcomeStatus::Verified => "verified",
        AnnotationOutcomeStatus::Failed => "failed",
        AnnotationOutcomeStatus::BuildFailed => "build_failed",
        AnnotationOutcomeStatus::Inconclusive => "inconclusive",
        AnnotationOutcomeStatus::NoCoverage => "no_coverage",
    }
}

fn test_status_word(s: TestOutcomeStatus) -> &'static str {
    match s {
        TestOutcomeStatus::Pass => "passed",
        TestOutcomeStatus::Fail => "failed",
        TestOutcomeStatus::BuildFailed => "build_failed",
        TestOutcomeStatus::CloneFailed => "clone_failed",
        TestOutcomeStatus::Timeout => "timeout",
        TestOutcomeStatus::Error => "error",
    }
}

fn print_session_dispatched(req: &VerifySessionRequest, resp: &PostVerifySessionResponse) {
    let annotation_word = if resp.plan_size == 1 {
        "annotation"
    } else {
        "annotations"
    };
    println!();
    println!(
        "→ verify session dispatched — verifying {} {annotation_word} against {}",
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
            "verify requires authentication: {e}\n  \
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
                "verify auth error: {inner}\n  \
                 Your token may be expired — re-run `aristo auth login`."
            ),
            exit_code: 1,
        },
        VerifyError::BadRequest {
            status: 402,
            message,
        } => CliError::Other {
            message: format!(
                "no canon coverage applies for your scopes — \
                 contact Aretta for DP onboarding to enable verification.\n  \
                 (server message: {message})"
            ),
            exit_code: 1,
        },
        VerifyError::BadRequest { status, message } => CliError::Other {
            message: format!("verify server rejected request (HTTP {status}): {message}"),
            exit_code: 1,
        },
        other => CliError::Other {
            message: format!("verify failed: {other}"),
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
    });
    // Pre-canned GET response sequence (for --view + --wait paths).
    let get_responses: Vec<GetVerifySessionResponse> = parsed.gets.unwrap_or_default();
    // Record the POST body to a sibling file so tests can inspect it.
    let record_path = format!("{path}.posted.json");
    let mock = match (post_resp, get_responses.is_empty()) {
        (Some(p), true) => aristo_core::canon_verify::MockVerifyClient::with_post_response(p),
        (Some(p), false) => {
            aristo_core::canon_verify::MockVerifyClient::with_post_and_gets(p, get_responses)
        }
        (None, false) => {
            aristo_core::canon_verify::MockVerifyClient::with_get_responses(get_responses)
        }
        (None, true) => return None,
    };
    Some(Box::new(RecordingMock {
        inner: mock,
        record_path,
    }))
}

#[derive(serde::Deserialize)]
struct FixtureFile {
    post: Option<FixturePost>,
    gets: Option<Vec<GetVerifySessionResponse>>,
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
        let mut f = CanonMatchesFile {
            meta: CacheMeta::default(),
            ..CanonMatchesFile::default()
        };
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
                    linked: None,
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
        let dispatch = vec![CanonDispatchEntry {
            id: &id,
            entry: &entry,
        }];

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
        let dispatch = vec![CanonDispatchEntry {
            id: &id,
            entry: &entry,
        }];
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
        let dispatch = vec![CanonDispatchEntry {
            id: &id,
            entry: &entry,
        }];
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
        let dispatch = vec![CanonDispatchEntry {
            id: &id,
            entry: &entry,
        }];
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
        let dispatch = vec![CanonDispatchEntry {
            id: &id,
            entry: &entry,
        }];
        let tags = build_tags(&dispatch, &matches);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].source_path, "src/foo.rs");
    }

    // ─── strip_canon_prefix ──────────────────────────────────────────────

    #[test]
    fn strip_canon_prefix_handles_both_tiers_and_no_prefix() {
        assert_eq!(strip_canon_prefix("aristos:foo_bar"), "foo_bar");
        assert_eq!(
            strip_canon_prefix("kanon:checkout_total_non_negative"),
            "checkout_total_non_negative"
        );
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
        let mock =
            aristo_core::canon_verify::MockVerifyClient::with_post_error(VerifyError::BadRequest {
                status: 402,
                message: "no_canon_coverage".into(),
            });
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
