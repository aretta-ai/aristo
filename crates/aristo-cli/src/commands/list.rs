//! `aristo list [--filter K=V]... [--json]` — flat enumeration.
//!
//! Default text output: one line per annotation, sorted alphabetically by
//! id, footer summary by kind. `--filter` clauses use the J2 unified
//! grammar (`id=`, `file=`, `parent=`, `status=`); multiple `--filter`
//! flags AND together. `--json` emits an array of records suitable for
//! piping into `jq` / scripts.

use aristo_core::index::{AnnotationId, IndexEntry, ParentLink};
use serde::Serialize;

use crate::commands::index::workspace_or_error;
use crate::commands::show::{read_index, status_label, verify_label};
use crate::filter::Filter;
use crate::{CliError, CliResult};

pub(crate) fn run(filter_strings: &[String], json: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let index = read_index(&ws.index_path())?;
    let filters = parse_filters(filter_strings)?;

    let matches: Vec<(&AnnotationId, &IndexEntry)> = index
        .entries
        .iter()
        .filter(|(id, e)| filters.iter().all(|f| matches_filter(id, e, f)))
        .collect();

    if json {
        emit_json(&matches)
    } else {
        emit_text(&matches, &filters, index.entries.len())
    }
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

fn matches_filter(id: &AnnotationId, entry: &IndexEntry, f: &Filter) -> bool {
    match f {
        Filter::Id(want) => id.as_str() == want,
        Filter::File(want) => match entry {
            IndexEntry::Intent(e) => e.file == *want,
            IndexEntry::Assume(e) => e.file == *want,
        },
        Filter::Parent(want) => match parent_of(entry) {
            Some(link) => link.iter().any(|p| p.as_str() == want),
            None => false,
        },
        Filter::Status(want) => status_label(status_of(entry)) == want,
    }
}

fn parent_of(entry: &IndexEntry) -> Option<&ParentLink> {
    match entry {
        IndexEntry::Intent(e) => e.parent.as_ref(),
        IndexEntry::Assume(e) => e.parent.as_ref(),
    }
}

fn status_of(entry: &IndexEntry) -> aristo_core::index::Status {
    match entry {
        IndexEntry::Intent(e) => e.status,
        IndexEntry::Assume(e) => e.status,
    }
}

// ─── Text rendering ────────────────────────────────────────────────────────

fn emit_text(
    matches: &[(&AnnotationId, &IndexEntry)],
    filters: &[Filter],
    total: usize,
) -> CliResult<()> {
    let id_col_width = matches
        .iter()
        .map(|(id, _)| id.as_str().chars().count())
        .max()
        .unwrap_or(0)
        .max(20);

    for (id, entry) in matches {
        let kind_label = match entry {
            IndexEntry::Intent(_) => "intent",
            IndexEntry::Assume(_) => "assume",
        };
        let verify = match entry {
            IndexEntry::Intent(e) => format!("verify={}", verify_label(e.verify)),
            IndexEntry::Assume(_) => "verify=-".to_string(),
        };
        let status = format!("status={}", status_label(status_of(entry)));
        let stale_marker = if matches!(status_of(entry), aristo_core::index::Status::Stale) {
            "  WARN"
        } else {
            ""
        };
        println!(
            "  {id:<width$}  {kind_label}  {verify:<13}  {status}{stale_marker}",
            id = id.as_str(),
            width = id_col_width,
        );
    }

    println!();
    if filters.is_empty() {
        let intent_count = matches
            .iter()
            .filter(|(_, e)| matches!(e, IndexEntry::Intent(_)))
            .count();
        let assume_count = matches.len() - intent_count;
        println!(
            "{} annotations  ({intent_count} intent / {assume_count} assume)",
            matches.len()
        );
    } else {
        let n = matches.len();
        let word = if n == 1 { "match" } else { "matches" };
        println!("{n} {word}.  ({total} total in index)");
    }
    Ok(())
}

// ─── JSON rendering ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ListRecord {
    id: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    verify: Option<String>,
    status: String,
    file: String,
    site: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<Vec<String>>,
}

fn emit_json(matches: &[(&AnnotationId, &IndexEntry)]) -> CliResult<()> {
    let records: Vec<ListRecord> = matches
        .iter()
        .map(|(id, e)| {
            let (kind, verify) = match e {
                IndexEntry::Intent(i) => ("intent", Some(verify_label(i.verify))),
                IndexEntry::Assume(_) => ("assume", None),
            };
            let parent = parent_of(e).map(|p| p.iter().map(|x| x.to_string()).collect());
            let (file, site) = match e {
                IndexEntry::Intent(i) => (i.file.clone(), i.site.clone()),
                IndexEntry::Assume(a) => (a.file.clone(), a.site.clone()),
            };
            ListRecord {
                id: id.to_string(),
                kind,
                verify,
                status: status_label(status_of(e)).to_string(),
                file,
                site,
                parent,
            }
        })
        .collect();
    let s = serde_json::to_string_pretty(&records).map_err(|e| CliError::Other {
        message: format!("serializing list as JSON: {e}"),
        exit_code: 1,
    })?;
    println!("{s}");
    Ok(())
}
