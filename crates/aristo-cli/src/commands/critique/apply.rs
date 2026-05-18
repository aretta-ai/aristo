//! `aristo critique --apply-findings` — scan `.aristo/critiques/*.critique`,
//! re-validate against the current index, print a human-readable summary.
//!
//! By default the per-id summary lists only **open** findings (those whose
//! `disposition` is `None`). Findings the user has already triaged via
//! `aristo session decide` carry a `disposition` (Accepted / Rejected /
//! Deferred) and stop re-surfacing on every apply — closing the review
//! loop the substrate is designed for. Pass `--include-closed` to opt
//! back into the full view; closed findings render with a `[disposition]`
//! label before their rationale.

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::critique::{CritiqueFile, Disposition, Finding, Severity};
use aristo_core::index::AnnotationId;

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

#[aristo::intent(
    "`aristo critique --apply-findings` defaults to listing only findings \
     whose `disposition` is `None` (open / not yet reviewed). Findings the \
     user has already accepted, rejected, or deferred via \
     `aristo session decide` stop re-surfacing on every apply — that's how \
     the review substrate closes the loop. A refactor that re-surfaces \
     every finding by default breaks the user's \"I already triaged this\" \
     assumption and re-introduces the noise the substrate exists to filter. \
     `--include-closed` is the explicit opt-back-in.",
    verify = "neural",
    id = "apply_findings_filters_open_by_default"
)]
pub(crate) fn run_apply_findings(_ws: &Workspace, include_closed: bool) -> CliResult<()> {
    // Re-resolve workspace + index here so we don't depend on caller
    // having loaded them already.
    let ws = workspace_or_error()?;
    let index = read_index(&ws.index_path())?;

    let critiques_dir = ws.aristo_dir().join("critiques");
    if !critiques_dir.is_dir() {
        println!("ok: no critique files in .aristo/critiques/.");
        return Ok(());
    }

    let mut accepted: Vec<(AnnotationId, CritiqueFile)> = Vec::new();
    let mut rejected: Vec<(PathBuf, String)> = Vec::new();
    let mut parse_errors: Vec<(PathBuf, String)> = Vec::new();

    for path in collect_critique_files(&critiques_dir)? {
        let Some(id) = id_from_filename(&path) else {
            parse_errors.push((path.clone(), "filename is not <id>.critique".into()));
            continue;
        };
        let raw = fs::read_to_string(&path).map_err(|e| CliError::Other {
            message: format!("read {}: {e}", path.display()),
            exit_code: 1,
        })?;
        let cf = match CritiqueFile::parse(&raw) {
            Ok(c) => c,
            Err(e) => {
                parse_errors.push((path.clone(), format!("parse: {e}")));
                continue;
            }
        };
        // Re-validate against current index (catches text drift since
        // submit). The error type is the same shape as submit-time.
        let report = super::validator::validate(&id, &cf, &index);
        if !report.is_empty() {
            rejected.push((path.clone(), report.render()));
            continue;
        }
        accepted.push((id, cf));
    }

    // Sweep queue stragglers for accepted ids — same pattern as verify's
    // apply path. Belt-and-suspenders: submit-findings already cleared
    // claimed/<id>.toml on success.
    let qdir = QueueDir::for_pipeline(&ws, super::pending::PIPELINE_NAME);
    for (id, _) in &accepted {
        queue::submit_done(&qdir, id)?;
        let pending_path = qdir.pending_path(id);
        if pending_path.is_file() {
            let _ = std::fs::remove_file(&pending_path);
        }
    }

    print_summary(&accepted, &rejected, &parse_errors, include_closed);

    if !rejected.is_empty() || !parse_errors.is_empty() {
        Err(CliError::Silent { exit_code: 1 })
    } else {
        Ok(())
    }
}

fn collect_critique_files(dir: &Path) -> CliResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| CliError::Other {
        message: format!("read_dir {}: {e}", dir.display()),
        exit_code: 1,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| CliError::Other {
            message: format!("read_dir entry: {e}"),
            exit_code: 1,
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("critique") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn id_from_filename(path: &Path) -> Option<AnnotationId> {
    let stem = path.file_stem()?.to_str()?;
    let id_str = stem.replace("__", ":");
    AnnotationId::parse(&id_str).ok()
}

fn print_summary(
    accepted: &[(AnnotationId, CritiqueFile)],
    rejected: &[(PathBuf, String)],
    parse_errors: &[(PathBuf, String)],
    include_closed: bool,
) {
    let total = accepted.len() + rejected.len() + parse_errors.len();
    if total == 0 {
        println!("ok: no critique files in .aristo/critiques/.");
        return;
    }
    println!(
        "applied: {}/{} critique(s) ({} rejected, {} unparseable).",
        accepted.len(),
        total,
        rejected.len(),
        parse_errors.len()
    );
    if !accepted.is_empty() {
        println!();
        for (id, cf) in accepted {
            let total_findings = cf.critique.findings.len();
            let (visible, total_closed) =
                partition_for_render(&cf.critique.findings, include_closed);

            let sev = cf
                .critique
                .highest_severity
                .map(severity_label)
                .unwrap_or("—");
            if total_findings == 0 {
                println!("  {id}  no findings");
            } else if visible.is_empty() {
                println!(
                    "  {id}  {total_findings} finding{} ({total_closed} closed, hidden — pass --include-closed to show)",
                    if total_findings == 1 { "" } else { "s" }
                );
            } else {
                let suffix = if total_closed > 0 && include_closed {
                    format!(" ({total_closed} closed)")
                } else if total_closed > 0 {
                    format!(" ({total_closed} closed, hidden)")
                } else {
                    String::new()
                };
                println!(
                    "  {id}  {} finding{} (highest: {sev}){suffix}",
                    visible.len(),
                    if visible.len() == 1 { "" } else { "s" }
                );
                for f in visible {
                    let disp = match f.disposition {
                        Some(d) => format!(
                            "[{}, {}] ",
                            category_label(f.category),
                            disposition_label(d)
                        ),
                        None => format!("[{}] ", category_label(f.category)),
                    };
                    println!("    {disp}{}", truncate(&f.rationale, 100));
                }
            }
        }
    }
    for (path, msg) in parse_errors {
        eprintln!("error: {}: {}", path.display(), msg);
    }
    for (path, msg) in rejected {
        eprintln!("error: {}: {}", path.display(), msg);
    }
}

/// Split findings into (visible-for-this-view, total-closed-count).
/// Default view (`include_closed = false`) hides findings with any
/// `disposition` — that's the load-bearing behavior pinned by intent
/// `apply_findings_filters_open_by_default`. `--include-closed` shows
/// everything; either way the caller gets the total count of closed
/// findings so it can render a `(N closed)` or `(N closed, hidden)`
/// suffix appropriate to the chosen view.
fn partition_for_render(findings: &[Finding], include_closed: bool) -> (Vec<&Finding>, usize) {
    let total_closed = findings.iter().filter(|f| f.disposition.is_some()).count();
    let visible: Vec<&Finding> = findings
        .iter()
        .filter(|f| include_closed || f.disposition.is_none())
        .collect();
    (visible, total_closed)
}

fn disposition_label(d: Disposition) -> &'static str {
    match d {
        Disposition::Accepted => "accepted",
        Disposition::Rejected => "rejected",
        Disposition::Deferred => "deferred",
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Suggest => "suggest",
        Severity::StrongSuggest => "strong-suggest",
    }
}

fn category_label(c: aristo_core::critique::Category) -> &'static str {
    use aristo_core::critique::Category::*;
    match c {
        Rephrasing => "rephrasing",
        ParentShape => "parent-shape",
        Vocabulary => "vocabulary",
        Scope => "scope",
        Clarity => "clarity",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::critique::Category;

    fn finding(disposition: Option<Disposition>) -> Finding {
        Finding {
            category: Category::Clarity,
            severity: Severity::Suggest,
            rationale: "x".into(),
            suggested_text: None,
            disposition,
            disposition_note: None,
            closed_at: None,
        }
    }

    #[test]
    fn partition_default_hides_findings_with_any_disposition() {
        let findings = vec![
            finding(None),
            finding(Some(Disposition::Accepted)),
            finding(Some(Disposition::Rejected)),
            finding(Some(Disposition::Deferred)),
            finding(None),
        ];
        let (visible, total_closed) = partition_for_render(&findings, false);
        assert_eq!(
            visible.len(),
            2,
            "only the 2 None-disposition entries visible"
        );
        assert!(visible.iter().all(|f| f.disposition.is_none()));
        assert_eq!(total_closed, 3);
    }

    #[test]
    fn partition_include_closed_shows_everything_with_full_closed_count() {
        let findings = vec![
            finding(None),
            finding(Some(Disposition::Accepted)),
            finding(Some(Disposition::Rejected)),
        ];
        let (visible, total_closed) = partition_for_render(&findings, true);
        assert_eq!(visible.len(), 3);
        assert_eq!(
            total_closed, 2,
            "total_closed is informational with --include-closed"
        );
    }

    #[test]
    fn partition_empty_findings_is_empty_zero() {
        let (visible, total_closed) = partition_for_render(&[], false);
        assert!(visible.is_empty());
        assert_eq!(total_closed, 0);
    }

    #[test]
    fn partition_all_closed_hides_all_by_default() {
        let findings = vec![
            finding(Some(Disposition::Accepted)),
            finding(Some(Disposition::Deferred)),
        ];
        let (visible, total_closed) = partition_for_render(&findings, false);
        assert!(visible.is_empty(), "all closed, none visible by default");
        assert_eq!(total_closed, 2);
    }
}
