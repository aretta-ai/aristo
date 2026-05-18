//! `aristo critique` — agentic prose review of annotations.
//!
//! Slice 27. Mirrors verify's CLI ↔ skill split (per
//! `docs/decisions/critique-and-pipeline-architecture.md`) but with three
//! per-pipeline differences:
//!
//! - **Self-contained queue entries** (D2): a critique task embeds the
//!   focal annotation text + sibling and parent texts. Workers do not
//!   read source files; they only read the popped task.
//! - **Sonnet workers** (D4): shallow prose-quality work; Opus is overkill.
//! - **Filter-required default** (D6): `aristo critique` with no `--filter`
//!   errors with usage guidance. No implicit codebase sweep.

use aristo_core::index::{AnnotationId, IndexEntry};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::filter::Filter;
use crate::pipeline;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::workspace::Workspace;
use crate::{CliError, CliResult};

pub(crate) mod apply;
pub(crate) mod pending;
pub(crate) mod session_kind;
pub(crate) mod submit;
pub(crate) mod validator;

#[aristo::intent(
    "`aristo critique` requires an explicit `--filter` (id or file). \
     Default scope is NOT all annotations — an unbounded codebase sweep \
     is an expensive LLM operation and shouldn't be the accidental path. \
     A refactor that defaults to `--all` would turn `aristo critique` \
     into a footgun the first time a user runs it on a large project.",
    verify = "neural",
    id = "critique_requires_explicit_filter_no_implicit_all"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "flag-shaped dispatcher; collapsing into a struct adds indirection without simplifying call sites — same pattern as verify::run"
)]
pub(crate) fn run(
    filter_strings: &[String],
    submit_findings: bool,
    pop_next: bool,
    queue_status: bool,
    apply_findings: bool,
    include_closed: bool,
    rerun: bool,
    staged: bool,
    all: bool,
    yes: bool,
    id: Option<String>,
    json: Option<String>,
) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    // Reads (pop_next, queue_status) bypass the session guard;
    // workers must keep functioning so an open session of any kind
    // doesn't strand in-flight critique dispatch. Writes block.
    if pop_next {
        return run_pop_next(&ws);
    }
    if queue_status {
        return run_queue_status(&ws);
    }

    if submit_findings {
        crate::session::guard::ensure_no_active_session(&ws, "aristo critique --submit-findings")?;
        let id_str = id.expect("--id is required with --submit-findings (enforced by clap)");
        let json_str = json.expect("--json is required with --submit-findings (enforced by clap)");
        return submit::run_submit_findings(&ws, &index, &id_str, &json_str);
    }

    if apply_findings {
        crate::session::guard::ensure_no_active_session(&ws, "aristo critique --apply-findings")?;
        return apply::run_apply_findings(&ws, include_closed);
    }

    // Default path: enqueue tasks for the filtered ids. `--staged` and
    // `--all` each satisfy the filter-required guard on their own.
    if filter_strings.is_empty() && !staged && !all {
        return Err(CliError::Other {
            message: "`aristo critique` requires `--filter`, `--staged`, or `--all`. Examples:\n  \
                 aristo critique --filter id=my_intent\n  \
                 aristo critique --filter id=foo,bar,baz\n  \
                 aristo critique --filter file=src/x.rs\n  \
                 aristo critique --staged    # critique annotations in git-staged files\n  \
                 aristo critique --all       # sweep every verifiable intent (loud confirmation prompt)\n\
                 (the sweep is intentionally loud — critique is an LLM call \
                 and shouldn't be the accidental path)"
                .into(),
            exit_code: 2,
        });
    }

    crate::session::guard::ensure_no_active_session(&ws, "aristo critique")?;

    let filters = parse_filters(filter_strings)?;
    let staged_files: Option<std::collections::BTreeSet<String>> = if staged {
        Some(staged_files_or_error(&ws)?)
    } else {
        None
    };
    let mut matched: Vec<&AnnotationId> = Vec::new();
    for (id, entry) in index.entries.iter() {
        if all {
            // --all sweeps every IntentEntry with a real verify method
            // (excludes documentation-only verify=false). Assumes are
            // skipped — they're not critique candidates per A5.
            if !is_critique_candidate(entry) {
                continue;
            }
        } else {
            if !matches_all(id, entry, &filters) {
                continue;
            }
            if let Some(staged) = &staged_files {
                if !staged.contains(file_of(entry)) {
                    continue;
                }
            }
        }
        matched.push(id);
    }

    // --all confirmation gate. Fires AFTER matching so the user sees the
    // real count, not an over- or under-estimate.
    if all && !yes {
        return Err(CliError::Other {
            message: format_all_confirmation(matched.len()),
            exit_code: 2,
        });
    }

    if matched.is_empty() {
        println!("ok: 0 annotations matched the filter; nothing to critique.");
        return Ok(());
    }

    // Skip ids whose .critique is current — text_hash matches the cached
    // last_critiqued_at_text_hash from `--apply-findings`. `--rerun`
    // bypasses the cache and re-enqueues unconditionally.
    let (targets, skipped) = if rerun {
        (matched, 0usize)
    } else {
        let total = matched.len();
        let kept: Vec<&AnnotationId> = matched
            .into_iter()
            .filter(|id| !critique_is_current(&index, id))
            .collect();
        let skipped = total - kept.len();
        (kept, skipped)
    };

    if skipped > 0 {
        println!(
            "→ {skipped} {} skipped (text unchanged since last critique; pass --rerun to force).",
            if skipped == 1 { "entry" } else { "entries" }
        );
    }

    if targets.is_empty() {
        println!("ok: nothing to enqueue.");
        return Ok(());
    }

    let enqueued = pending::enqueue_pending(&ws, &index, &targets)?;
    println!(
        "→ {enqueued} {} enqueued for critique under .aristo/critique-queue/pending/.",
        if enqueued == 1 { "entry" } else { "entries" }
    );
    println!("  In Claude Code (or another agent with the aristo-critique skill installed), run:");
    println!("    /aristo-critique");
    println!(
        "  to produce findings for each pending entry. The skill writes .aristo/critiques/<id>.critique"
    );
    println!("  files; run `aristo critique --apply-findings` to validate and surface them.");
    Ok(())
}

fn run_pop_next(ws: &Workspace) -> CliResult<()> {
    let qdir = pipeline::queue::QueueDir::for_pipeline(ws, pending::PIPELINE_NAME);
    match pipeline::queue::pop_next(&qdir)? {
        Some(task) => {
            print!("{}", task.content);
            Ok(())
        }
        None => Ok(()),
    }
}

fn run_queue_status(ws: &Workspace) -> CliResult<()> {
    let qdir = pipeline::queue::QueueDir::for_pipeline(ws, pending::PIPELINE_NAME);
    let status = pipeline::queue::queue_status(&qdir)?;
    println!("pending: {}", status.pending);
    println!("claimed: {}", status.claimed);
    Ok(())
}

fn parse_filters(filter_strings: &[String]) -> CliResult<Vec<Filter>> {
    let mut out = Vec::with_capacity(filter_strings.len());
    for raw in filter_strings {
        let f: Filter = raw.parse().map_err(|e| CliError::Other {
            message: format!("{e}"),
            exit_code: 2,
        })?;
        out.push(f);
    }
    Ok(out)
}

fn matches_all(id: &AnnotationId, entry: &IndexEntry, filters: &[Filter]) -> bool {
    filters.iter().all(|f| matches_filter(id, entry, f))
}

fn matches_filter(id: &AnnotationId, entry: &IndexEntry, f: &Filter) -> bool {
    match f {
        Filter::Id(want) => id.as_str() == want,
        Filter::File { path, line_range } => {
            if file_of(entry) != path {
                return false;
            }
            match line_range {
                None => true,
                Some((lo, hi)) => match site_line(entry) {
                    Some(line) => line >= *lo && line <= *hi,
                    None => false,
                },
            }
        }
        Filter::Parent(want) => match parent_ids(entry) {
            Some(ids) => ids.iter().any(|p| p.as_str() == want),
            None => false,
        },
        Filter::Status(want) => crate::commands::show::status_label(status_of(entry)) == want,
    }
}

/// Parse the annotation's site-line number out of the stamped `site`
/// string. Format is `"<site_text> (line N)"` (see commands/index.rs's
/// IntentEntry construction). Returns `None` when the suffix doesn't
/// match — keeps the line-range filter strict (an unparseable site
/// excludes the entry rather than letting it through silently).
fn site_line(entry: &IndexEntry) -> Option<u32> {
    let site = match entry {
        IndexEntry::Intent(e) => &e.site,
        IndexEntry::Assume(e) => &e.site,
    };
    // The suffix is exactly " (line <N>)" — find the last "(line " and
    // parse the number before the trailing `)`.
    let open = site.rfind("(line ")?;
    let after_open = &site[open + "(line ".len()..];
    let close = after_open.rfind(')')?;
    after_open[..close].trim().parse().ok()
}

fn file_of(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn status_of(entry: &IndexEntry) -> aristo_core::index::Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

fn parent_ids(entry: &IndexEntry) -> Option<&aristo_core::index::ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

#[aristo::intent(
    "`aristo critique --all` requires explicit confirmation via `--yes` \
     (or interactive Y/N — interactive is parked for v2; v1 requires \
     `--yes` on the command line). The cost-gate fires AFTER the matched \
     set is computed so the count and dollar estimate match what the \
     user is actually about to pay for. A refactor that proceeds without \
     `--yes` would let an agent (or a script) accidentally enqueue \
     hundreds of LLM calls in one bash invocation — the gate exists \
     because critique is the most expensive aristo operation per token \
     spent.",
    verify = "neural",
    id = "critique_all_flag_requires_confirmation_or_yes"
)]
fn format_all_confirmation(n: usize) -> String {
    // Fixed-per-task heuristic: ~$0.01 per Sonnet critique task (input
    // ~2KB context + output ~500 tokens at current Sonnet pricing).
    // Refinement parked: real-data calibration after v0.1.0 dogfood.
    let est_cost_dollars = (n as f64) * 0.01;
    format!(
        "`aristo critique --all` will enqueue {n} annotation{} (~${est_cost_dollars:.2} estimated LLM cost).\n\
         This is a no-op preview — pass `--yes` to actually enqueue:\n  \
             aristo critique --all --yes",
        if n == 1 { "" } else { "s" }
    )
}

fn is_critique_candidate(entry: &IndexEntry) -> bool {
    // Critique candidates are IntentEntries with a real verify method.
    // Assumes (per A5) are documentation-only and not critique targets;
    // intents with `verify = false` are also documentation-only.
    match entry {
        IndexEntry::Intent(e) => match e.verify {
            aristo_core::index::VerifyLevel::Bool(false) => false,
            aristo_core::index::VerifyLevel::Bool(true) => true,
            aristo_core::index::VerifyLevel::Method(_) => true,
        },
        IndexEntry::Assume(_) => false,
    }
}

#[aristo::intent(
    "`aristo critique --staged` intersects with any `--filter file=` \
     clauses rather than replacing them. Composition semantic, not \
     replacement: `--staged --filter file=src/x.rs` enqueues the \
     intersection (annotations in src/x.rs that ALSO appear in the \
     git-staged set), not the union. A refactor that unions them would \
     turn `--staged` into a quiet expansion that ignores explicit \
     scoping the user added, contradicting its scoping-tighter purpose. \
     Empty intersection (no staged files match the filter) yields the \
     usual `0 annotations matched` exit, not an error.",
    verify = "neural",
    id = "critique_staged_filter_intersects_with_explicit_filter"
)]
fn staged_files_or_error(ws: &Workspace) -> CliResult<std::collections::BTreeSet<String>> {
    let output = std::process::Command::new("git")
        .arg("diff")
        .arg("--cached")
        .arg("--name-only")
        .current_dir(&ws.root)
        .output()
        .map_err(|e| CliError::Other {
            message: format!("`--staged` requires `git` on PATH; running `git diff --cached --name-only` failed: {e}"),
            exit_code: 2,
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::Other {
            message: format!(
                "`--staged` ran `git diff --cached --name-only` but git exited non-zero. \
                 Is this directory inside a git repository? Stderr: {stderr}"
            ),
            exit_code: 2,
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: std::collections::BTreeSet<String> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect();
    Ok(files)
}

#[aristo::intent(
    "`aristo critique` consults `last_critiqued_at_text_hash` on each \
     IntentEntry before re-enqueueing it. If the cached value equals the \
     entry's current `text_hash`, the existing .critique file is up to \
     date and re-running the LLM would burn tokens for the same answer — \
     skip the enqueue. This is the whole point of the cache: a refactor \
     that always re-enqueues regardless of cache state re-introduces the \
     daily-loop LLM cost the cache exists to amortize. AssumeEntries and \
     entries with no cache (the field is `None`) are NEVER skipped — for \
     assumes the cache is irrelevant; for first-time entries the absent \
     cache means \"no critique on record\" so the dispatcher must produce \
     one. `--rerun` bypasses the cache entirely.",
    verify = "neural",
    id = "critique_skip_consults_cached_text_hash_for_drift"
)]
fn critique_is_current(index: &aristo_core::index::IndexFile, id: &AnnotationId) -> bool {
    let Some(IndexEntry::Intent(e)) = index.entries.get(id) else {
        return false;
    };
    match &e.last_critiqued_at_text_hash {
        Some(cached) => cached == &e.text_hash,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AssumeEntry, BindingState, CoveredRegion, IndexFile, IntentEntry, Meta, Sha256, Status,
        VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent_with_cache(text_hash: Sha256, cached: Option<Sha256>) -> IntentEntry {
        IntentEntry {
            text: "x".into(),
            verify: VerifyLevel::Method(VerifyMethod::Neural),
            status: Status::Unknown,
            text_hash,
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: BindingState::Local,
            parent: None,
            last_critiqued_at_text_hash: cached,
            last_critique_finding_count: None,
        }
    }

    fn one_intent_index(id_str: &str, e: IntentEntry) -> (AnnotationId, IndexFile) {
        let id = AnnotationId::parse(id_str).unwrap();
        let mut entries = BTreeMap::new();
        entries.insert(id.clone(), IndexEntry::Intent(e));
        (
            id,
            IndexFile {
                meta: Meta {
                    schema_version: 1,
                    generated_by: None,
                    generated_at: None,
                    source_root: None,
                },
                entries,
            },
        )
    }

    #[test]
    fn current_when_cache_matches_text_hash() {
        let text = sha('a');
        let (id, index) = one_intent_index("foo", intent_with_cache(text.clone(), Some(text)));
        assert!(critique_is_current(&index, &id));
    }

    #[test]
    fn not_current_when_cache_differs_from_text_hash() {
        // The annotation's text drifted since the last critique was produced.
        let (id, index) = one_intent_index("foo", intent_with_cache(sha('a'), Some(sha('e'))));
        assert!(!critique_is_current(&index, &id));
    }

    #[test]
    fn not_current_when_no_cache_recorded() {
        // First-time entry — last_critiqued_at_text_hash is None.
        let (id, index) = one_intent_index("foo", intent_with_cache(sha('a'), None));
        assert!(!critique_is_current(&index, &id));
    }

    #[test]
    fn not_current_for_assume_entries() {
        // Cache fields are IntentEntry-only. Assumes always re-enqueue
        // (though in practice the filter rarely selects them; per A5
        // assumes are documentation-only).
        let id = AnnotationId::parse("foo").unwrap();
        let assume = AssumeEntry {
            text: "x".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/x.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        };
        let mut entries = BTreeMap::new();
        entries.insert(id.clone(), IndexEntry::Assume(assume));
        let index = IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries,
        };
        assert!(!critique_is_current(&index, &id));
    }
}
