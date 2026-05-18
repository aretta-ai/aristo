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
pub(crate) fn run(
    filter_strings: &[String],
    submit_findings: bool,
    pop_next: bool,
    queue_status: bool,
    apply_findings: bool,
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
        return apply::run_apply_findings(&ws);
    }

    // Default path: enqueue tasks for the filtered ids.
    if filter_strings.is_empty() {
        return Err(CliError::Other {
            message: "`aristo critique` requires `--filter`. Examples:\n  \
                 aristo critique --filter id=my_intent\n  \
                 aristo critique --filter id=foo,bar,baz\n  \
                 aristo critique --filter file=src/x.rs\n\
                 (a default --all sweep is intentionally not provided — \
                 critique is an LLM call and shouldn't be the accidental path)"
                .into(),
            exit_code: 2,
        });
    }

    crate::session::guard::ensure_no_active_session(&ws, "aristo critique")?;

    let filters = parse_filters(filter_strings)?;
    let mut targets: Vec<&AnnotationId> = Vec::new();
    for (id, entry) in index.entries.iter() {
        if matches_all(id, entry, &filters) {
            targets.push(id);
        }
    }

    if targets.is_empty() {
        println!("ok: 0 annotations matched the filter; nothing to critique.");
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
        Filter::File(want) => file_of(entry) == want,
        Filter::Parent(want) => match parent_ids(entry) {
            Some(ids) => ids.iter().any(|p| p.as_str() == want),
            None => false,
        },
        Filter::Status(want) => crate::commands::show::status_label(status_of(entry)) == want,
    }
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
