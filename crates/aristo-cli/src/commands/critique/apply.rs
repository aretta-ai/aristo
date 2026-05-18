//! `aristo critique --apply-findings` — scan `.aristo/critiques/*.critique`,
//! re-validate against the current index, print a human-readable summary.
//!
//! For v0 of slice 27 this is print-only — no index update. The findings
//! files on disk are the source of truth; v1 will add
//! `last_critiqued_at_text_hash` + `last_critique_finding_count` +
//! `last_critique_highest_severity` to the index for caching.

use std::fs;
use std::path::{Path, PathBuf};

use aristo_core::critique::{CritiqueFile, Severity};
use aristo_core::index::AnnotationId;

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::pipeline::queue::{self, QueueDir};
use crate::{CliError, CliResult, Workspace};

#[aristo::intent(
    "`aristo critique --apply-findings` scans every `.critique` file, \
     re-validates against the current index (catches drift between \
     submit time and apply time), prints a per-id summary grouped by \
     severity, and sweeps queue stragglers. For v0 this is a read + \
     summary pass — index update fields (last_critiqued_at_text_hash, \
     last_critique_finding_count, last_critique_highest_severity) are \
     deferred to v1 so this slice ships without touching the IndexEntry \
     serde shape. The .critique files on disk are authoritative; \
     re-running `aristo critique --apply-findings` is idempotent and \
     non-destructive.",
    verify = "neural",
    id = "critique_apply_is_summary_only_in_v0"
)]
pub(crate) fn run_apply_findings(_ws: &Workspace) -> CliResult<()> {
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

    print_summary(&accepted, &rejected, &parse_errors);

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
            let n = cf.critique.findings.len();
            let sev = cf
                .critique
                .highest_severity
                .map(severity_label)
                .unwrap_or("—");
            if n == 0 {
                println!("  {id}  no findings");
            } else {
                println!(
                    "  {id}  {n} finding{} (highest: {sev})",
                    if n == 1 { "" } else { "s" }
                );
                for f in &cf.critique.findings {
                    println!(
                        "    [{}] {}",
                        category_label(f.category),
                        truncate(&f.rationale, 100)
                    );
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
        out.push_str("…");
        out
    }
}
