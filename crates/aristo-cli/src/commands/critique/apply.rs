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

use aristo_core::canon::{CanonMatchesFile, Disposition as CanonDisposition};
use aristo_core::critique::{CritiqueFile, Disposition, Finding, Severity};
use aristo_core::index::{AnnotationId, IndexEntry, IndexFile};

use crate::commands::index::{atomic_write, workspace_or_error};
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

    // Stamp the cache fields on every accepted critique's IntentEntry.
    // Commit 4 will consult these to skip re-enqueue when text hasn't
    // drifted. Assume entries are skipped (cache fields are IntentEntry-only).
    if !accepted.is_empty() {
        stamp_critique_cache(&ws, &index, &accepted)?;
    }

    print_summary(&accepted, &rejected, &parse_errors, include_closed);

    // Synthesize canonicalize findings from `.aristo/canon-matches.toml`
    // per README L4 — these are surfaced alongside the agentic
    // critique findings even though they live in a different file.
    // PR #7's accept path will turn user-accepted canonicalize
    // findings into the source-rewrite + prefix-application binding.
    print_canonicalize_findings(&ws, include_closed);

    if !rejected.is_empty() || !parse_errors.is_empty() {
        Err(CliError::Silent { exit_code: 1 })
    } else {
        Ok(())
    }
}

#[aristo::intent(
    "Canonicalize findings are surfaced alongside agentic critique \
     findings in `aristo critique --apply-findings` per README L4 — \
     they originate from `.aristo/canon-matches.toml::pending_matches`, \
     not from the `.critique` files the other five categories live in. \
     A regression that read only the .critique files would silently \
     hide every canon match the user has open for review.",
    verify = "test",
    id = "apply_findings_surfaces_canonicalize_from_canon_matches"
)]
fn print_canonicalize_findings(ws: &Workspace, include_closed: bool) {
    let cache_path = ws.canon_matches_path();
    if !cache_path.exists() {
        return;
    }
    let cache = match CanonMatchesFile::read(&cache_path) {
        Ok(c) => c,
        Err(_) => {
            // Mirror the .critique parse-error policy: don't fail
            // the apply just because the cache is malformed; surface
            // a stderr note. Stamp will re-write the cache on its
            // next successful run.
            eprintln!(
                "warning: {} is unreadable; canonicalize findings skipped.",
                cache_path.display()
            );
            return;
        }
    };

    // Render in two passes for consistency with print_summary's
    // open-vs-closed view: collect first, then decide what to print.
    let mut open_findings: Vec<(&AnnotationId, &aristo_core::canon::PendingMatch)> = Vec::new();
    let mut closed_count: usize = 0;
    for (id, entry) in &cache.entries {
        for m in &entry.pending_matches {
            match m.disposition {
                CanonDisposition::Open => open_findings.push((id, m)),
                CanonDisposition::Skipped => closed_count += 1,
            }
        }
    }
    if open_findings.is_empty() && (!include_closed || closed_count == 0) {
        return;
    }

    println!();
    println!(
        "canonicalize: {} open{}",
        open_findings.len(),
        if closed_count > 0 && include_closed {
            format!(", {closed_count} skipped")
        } else if closed_count > 0 {
            format!(" ({closed_count} skipped, hidden)")
        } else {
            String::new()
        }
    );
    for (id, m) in open_findings {
        let tier = match m.prefix_tier {
            aristo_core::canon::PrefixTier::Aristos => "aristos:",
            aristo_core::canon::PrefixTier::Kanon => "kanon:",
        };
        println!(
            "  {id}  → {} {} (conf {:.2}, {tier} tier)",
            m.canon_id, m.version, m.confidence
        );
        if let Some(b) = &m.backed_by {
            println!("    backed by: {b}");
        }
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

#[aristo::intent(
    "`aristo critique --apply-findings` stamps `last_critiqued_at_text_hash` \
     + `last_critique_finding_count` on each accepted critique's IntentEntry \
     in the same operation that validates the .critique file. The cache and \
     the on-disk .critique MUST be updated together: a writer that stamps \
     before validate or skips the stamp leaves readers seeing stale-cache + \
     fresh-file divergence (the cache claims a critique is current when the \
     file says otherwise, or vice versa). Idempotent: re-running on an \
     unchanged .critique re-writes the same values. AssumeEntry is skipped \
     (the cache fields are IntentEntry-only by design — assumes are \
     documentation-only annotations per A5 and don't have the same cache \
     lifecycle as verifiable intents).",
    verify = "neural",
    id = "critique_apply_stamps_cache_on_success"
)]
fn stamp_critique_cache(
    ws: &Workspace,
    index: &IndexFile,
    accepted: &[(AnnotationId, CritiqueFile)],
) -> CliResult<()> {
    let (new_index, stamped) = apply_cache_to_index(index.clone(), accepted);
    if stamped == 0 {
        return Ok(());
    }
    let toml_text = toml::to_string_pretty(&new_index).map_err(|e| CliError::Other {
        message: format!("serializing index.toml: {e}"),
        exit_code: 1,
    })?;
    atomic_write(&ws.index_path(), &toml_text)
}

/// Pure: write the cache fields onto each IntentEntry whose id appears
/// in `accepted`. Returns the mutated index + the count of entries stamped.
/// AssumeEntries are silently skipped (cache fields are IntentEntry-only).
/// Missing ids are silently skipped (the .critique points at an id that
/// no longer exists in the current index — apply already rejected those
/// upstream via the validator's focal_id check).
fn apply_cache_to_index(
    mut index: IndexFile,
    accepted: &[(AnnotationId, CritiqueFile)],
) -> (IndexFile, usize) {
    let mut stamped = 0usize;
    for (id, cf) in accepted {
        if let Some(IndexEntry::Intent(e)) = index.entries.get_mut(id) {
            e.last_critiqued_at_text_hash = Some(cf.critique.critiqued_at_text_hash.clone());
            e.last_critique_finding_count = Some(cf.critique.findings.len() as u32);
            stamped += 1;
        }
    }
    (index, stamped)
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
        Canonicalize => "canonicalize",
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
    use aristo_core::critique::{Category, CritiqueBody};
    use aristo_core::index::{
        AssumeEntry, BindingState, CoveredRegion, IntentEntry, Meta, Sha256, Status, VerifyLevel,
        VerifyMethod,
    };
    use std::collections::BTreeMap;

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

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent_entry(text_hash: Sha256) -> IntentEntry {
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
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
        }
    }

    fn empty_meta() -> Meta {
        Meta {
            schema_version: 1,
            generated_by: None,
            generated_at: None,
            source_root: None,
        }
    }

    fn critique_with(text_hash: Sha256, n_findings: usize) -> CritiqueFile {
        let findings = (0..n_findings).map(|_| finding(None)).collect();
        CritiqueFile {
            critique: CritiqueBody {
                critiqued_at_text_hash: text_hash,
                produced_at_body_hash: sha('c'),
                produced_by: "test".into(),
                attempts: 1,
                finding_count: Some(n_findings as u32),
                highest_severity: None,
                findings,
            },
        }
    }

    #[test]
    fn apply_cache_stamps_text_hash_and_count_on_intent() {
        let id = AnnotationId::parse("foo").unwrap();
        let text_hash = sha('a');
        let mut entries = BTreeMap::new();
        entries.insert(
            id.clone(),
            IndexEntry::Intent(intent_entry(text_hash.clone())),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let cf = critique_with(text_hash.clone(), 3);

        let (new_index, stamped) = apply_cache_to_index(index, &[(id.clone(), cf)]);
        assert_eq!(stamped, 1);

        let IndexEntry::Intent(e) = new_index.entries.get(&id).unwrap() else {
            panic!("expected intent");
        };
        assert_eq!(e.last_critiqued_at_text_hash.as_ref(), Some(&text_hash));
        assert_eq!(e.last_critique_finding_count, Some(3));
    }

    #[test]
    fn apply_cache_idempotent_on_repeat() {
        // Stamping twice with the same critique writes the same values.
        let id = AnnotationId::parse("foo").unwrap();
        let text_hash = sha('a');
        let mut entries = BTreeMap::new();
        entries.insert(
            id.clone(),
            IndexEntry::Intent(intent_entry(text_hash.clone())),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let cf = critique_with(text_hash.clone(), 2);

        let (idx1, _) = apply_cache_to_index(index, &[(id.clone(), cf.clone())]);
        let (idx2, stamped) = apply_cache_to_index(idx1.clone(), &[(id.clone(), cf)]);
        assert_eq!(stamped, 1, "second stamp still counts as a stamp");
        assert_eq!(idx1.entries, idx2.entries, "values unchanged on re-stamp");
    }

    #[test]
    fn apply_cache_skips_assume_entries() {
        // Cache fields are IntentEntry-only; an assume in `accepted` is a no-op.
        let id = AnnotationId::parse("foo").unwrap();
        let text_hash = sha('a');
        let assume = AssumeEntry {
            text: "x".into(),
            status: Status::Unknown,
            text_hash: text_hash.clone(),
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
            meta: empty_meta(),
            entries,
        };
        let cf = critique_with(text_hash, 1);

        let (_, stamped) = apply_cache_to_index(index, &[(id, cf)]);
        assert_eq!(stamped, 0, "assume entries are not stamped");
    }

    #[test]
    fn apply_cache_skips_unknown_ids() {
        // A .critique whose id no longer exists in the index is silently
        // skipped (the validator would've rejected it upstream anyway).
        let id_in = AnnotationId::parse("present").unwrap();
        let id_missing = AnnotationId::parse("gone").unwrap();
        let text_hash = sha('a');
        let mut entries = BTreeMap::new();
        entries.insert(
            id_in.clone(),
            IndexEntry::Intent(intent_entry(text_hash.clone())),
        );
        let index = IndexFile {
            meta: empty_meta(),
            entries,
        };
        let cf_present = critique_with(text_hash.clone(), 2);
        let cf_missing = critique_with(text_hash, 5);

        let (new_index, stamped) = apply_cache_to_index(
            index,
            &[(id_in.clone(), cf_present), (id_missing, cf_missing)],
        );
        assert_eq!(stamped, 1, "only the present id stamps");
        let IndexEntry::Intent(e) = new_index.entries.get(&id_in).unwrap() else {
            panic!("expected intent");
        };
        assert_eq!(e.last_critique_finding_count, Some(2));
    }
}
