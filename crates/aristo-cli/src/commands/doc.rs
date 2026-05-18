//! `aristo doc` — generate per-annotation markdown to `.aristo/doc/`.
//!
//! Reads `.aristo/index.toml`, renders each annotation as a small markdown
//! file at `.aristo/doc/<id-safe>.md` where `<id-safe>` replaces `:` with
//! `__` for filesystem safety (same convention as `.proof` / `.critique`
//! files). Intended for inclusion in user rustdoc via the `aristo_doc`
//! cargo feature (slice 30) or a hand-written `#[doc = include_str!(...)]`
//! attribute. See `../aretta-sdk/docs/mockups/10-doc-and-graph/samples.md`
//! for the per-annotation MD shape and `_summary.md` shape.
//!
//! Flag matrix per the I1 mockup:
//! - bare `aristo doc`: per-annotation MD only (no status block; no summary).
//! - `--summary`: write `_summary.md` only (no per-annotation pass).
//! - `--include-status`: bake current B5b status into rendered MD.
//! - `--check`: CI gate — recompute expected MD, diff against disk, non-zero on drift.
//!
//! Slice 28 ships `--summary` first (this commit). Per-annotation,
//! `--include-status`, `--check`, and incremental-skip land in subsequent
//! slice-28 commits.

use std::fs;
use std::path::Path;

use aristo_core::index::{
    AnnotationId, AssumeEntry, BindingState, IdNamespace, IndexEntry, IndexFile, IntentEntry,
    ParentLink, Status, VerifyLevel, VerifyMethod,
};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult, Workspace};

pub(crate) fn run(summary: bool, include_status: bool, check: bool) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    if summary {
        return run_summary(&ws, &index);
    }

    if check {
        return run_check(&ws, &index, include_status);
    }

    run_per_annotation(&ws, &index, include_status)
}

// ─── --check CI gate ───────────────────────────────────────────────────────

#[aristo::intent(
    "`aristo doc --check` is a CI gate: it MUST NOT write to \
     `.aristo/doc/` under any circumstance — its job is to detect \
     drift so CI can block a merge that has stale doc artifacts. A \
     regression that wrote during --check would silently fix the \
     thing CI was supposed to catch.",
    verify = "neural",
    id = "doc_check_never_writes"
)]
fn run_check(ws: &Workspace, index: &IndexFile, include_status: bool) -> CliResult<()> {
    let doc_dir = ws.root.join(".aristo").join("doc");

    println!("→ Reading .aristo/index.toml … ok");
    println!("→ Computing expected per-annotation markdown …");
    println!("→ Comparing against .aristo/doc/ on disk …");

    let mut drift: Vec<String> = Vec::new();
    for (id, entry) in &index.entries {
        let path = doc_dir.join(format!("{}.md", id_safe(id)));
        let rendered = render_annotation_md(id, entry, include_status);
        let on_disk = fs::read_to_string(&path).unwrap_or_default();
        if on_disk != rendered {
            drift.push(id_safe(id));
        }
    }

    if drift.is_empty() {
        println!();
        println!("ok: doc artifacts are in sync with the index.");
        return Ok(());
    }

    for d in &drift {
        println!("  • Out of sync: .aristo/doc/{d}.md");
        println!("    (text in index does not match rendered markdown)");
    }
    println!();

    let n = drift.len();
    let plural = if n == 1 { "" } else { "s" };
    Err(CliError::Other {
        message: format!(
            "{n} doc artifact{plural} out of sync with the index.\n\
             \x20      Run `aristo doc` locally and commit the result."
        ),
        exit_code: 1,
    })
}

// ─── per-annotation rendering ──────────────────────────────────────────────

#[aristo::intent(
    "`aristo doc` writes each annotation to .aristo/doc/<id-safe>.md \
     where `<id-safe>` substitutes `:` with `__`. Same convention as \
     `.proof` and `.critique` files so users have one mental model for \
     id↔filename mapping across the SDK. A regression that picks a \
     different escape (or uses the raw id with `:`) would create \
     platform-specific filename failures (`:` is illegal on Windows / \
     macOS HFS+) AND silently break the slice-30 proc-macro that \
     reads these files via `include_str!`.",
    verify = "neural",
    id = "doc_per_annotation_filename_uses_id_safe"
)]
#[aristo::intent(
    "`aristo doc` output shape differs by first-run-vs-incremental: \
     first run (empty .aristo/doc/) prints per-file `• Wrote:` lines, \
     a `(N files written, 0 unchanged)` count, and the `Next steps` \
     onboarding footer; subsequent runs (any pre-existing file) print \
     `• Updated:`/`• Unchanged:` lines and a compressed \
     `ok: doc artifacts updated. (M written, N unchanged)` summary \
     with no onboarding footer. The pivot is whether the doc dir was \
     empty before the run, not whether any file was unchanged this \
     time — a regression that switched to the count-based check \
     would emit onboarding footers on every run that happens to \
     write all files (e.g. a schema upgrade that touches every MD).",
    verify = "neural",
    id = "doc_output_shape_pivots_on_empty_doc_dir_not_per_run_counts"
)]
fn run_per_annotation(ws: &Workspace, index: &IndexFile, include_status: bool) -> CliResult<()> {
    let doc_dir = ws.root.join(".aristo").join("doc");
    let is_first_run = doc_dir
        .read_dir()
        .map(|mut d| d.next().is_none())
        .unwrap_or(true);
    fs::create_dir_all(&doc_dir).map_err(CliError::Io)?;

    let counts = Counts::from(index);
    println!();
    if include_status {
        println!(
            "→ Reading .aristo/index.toml … ok ({} annotations)",
            counts.total,
        );
        println!("→ Including current B5b verification status in rendered docs.");
        println!("→ Generating per-annotation markdown to .aristo/doc/ …");
    } else if is_first_run {
        println!(
            "→ Reading .aristo/index.toml … ok ({} annotations: {} intent, {} assume)",
            counts.total, counts.intent, counts.assume,
        );
        println!("→ Generating per-annotation markdown to .aristo/doc/ …");
    } else {
        println!(
            "→ Reading .aristo/index.toml … ok ({} annotations)",
            counts.total,
        );
        println!("→ Generating per-annotation markdown …");
    }

    let mut new_count = 0usize;
    let mut updated_count = 0usize;
    let mut unchanged_count = 0usize;
    for (id, entry) in &index.entries {
        let path = doc_dir.join(format!("{}.md", id_safe(id)));
        let already_exists = path.exists();
        let rendered = render_annotation_md(id, entry, include_status);
        if file_unchanged(&path, &rendered) {
            unchanged_count += 1;
            continue;
        }
        fs::write(&path, &rendered).map_err(CliError::Io)?;
        if include_status {
            // include-status mode: silent per-file (compressed output)
        } else if !already_exists {
            println!("  • Wrote: .aristo/doc/{}.md", id_safe(id));
            new_count += 1;
        } else {
            println!(
                "  • Updated: .aristo/doc/{}.md  (text changed)",
                id_safe(id)
            );
            updated_count += 1;
        }
    }
    let written = new_count + updated_count;

    if include_status {
        // include-status: also count writes that came from the silent branch
        let total_writes = index.entries.len() - unchanged_count;
        println!("  ({total_writes} files written, including status blocks)");
    } else if is_first_run {
        println!("  ({written} files written, {unchanged_count} unchanged)");
    } else if unchanged_count > 0 {
        println!("  • Unchanged: {unchanged_count} files");
    }
    println!();
    if include_status {
        println!("ok: doc artifacts updated.");
        println!();
        println!("ℹ Status is a build-time fact and will become stale as code evolves.");
        println!("   Re-run `aristo doc --include-status` to refresh, or omit `--include-status`");
        println!("   for purely static rendering.");
    } else if is_first_run {
        println!("ok: doc artifacts updated.");
        println!();
        println!("Next steps:");
        println!("  • Enable the `aristo_doc` cargo feature in your Cargo.toml:");
        println!("        [features]");
        println!("        default = [\"aristo_doc\"]");
        println!("  • Or run `cargo doc --features aristo_doc` to render docs with annotations.");
        println!("  • Optional: add `#[doc = include_str!(\".aristo/doc/_summary.md\")]`");
        println!("    above your crate's `//!` doc to render the project-level summary.");
    } else {
        println!("ok: doc artifacts updated. ({written} written, {unchanged_count} unchanged)");
    }
    Ok(())
}

/// True iff the file at `path` exists AND its current content equals
/// `rendered`. Drives the incremental-skip behavior promised by the I1
/// `--check` and incremental-run scenarios — a no-op re-write would
/// churn mtimes and bloat git diffs without changing the content.
fn file_unchanged(path: &Path, rendered: &str) -> bool {
    fs::read_to_string(path)
        .map(|on_disk| on_disk == rendered)
        .unwrap_or(false)
}

#[aristo::intent(
    "Per-annotation markdown structure is locked by the I1 `samples.md` \
     mockup: header line (`**Aristo verified intent — \\`<id>\\`**` for \
     intents, `**Aristo assumption — \\`<id>\\`**` for assumes), blank \
     line, body text verbatim, blank line, `<sub>` metadata line, \
     blank line, `---`. The metadata line composes verify-level + \
     server-bound marker + parent link with ` · ` separators. A \
     regression that drops the trailing `---` would break readers \
     that include this MD with `include_str!` between other doc \
     blocks — the separator is what isolates this annotation from \
     surrounding rustdoc.",
    verify = "neural",
    id = "doc_per_annotation_md_shape_locked_by_samples_mockup"
)]
fn render_annotation_md(id: &AnnotationId, entry: &IndexEntry, include_status: bool) -> String {
    let header = match entry {
        IndexEntry::Intent(_) => format!("**Aristo verified intent — `{id}`**"),
        IndexEntry::Assume(_) => format!("**Aristo assumption — `{id}`**"),
    };
    let text = match entry {
        IndexEntry::Intent(e) => e.text.as_str(),
        IndexEntry::Assume(e) => e.text.as_str(),
    };
    let meta = match entry {
        IndexEntry::Intent(e) => intent_meta_line(id, e),
        IndexEntry::Assume(e) => assume_meta_line(e),
    };
    let status_block = if include_status {
        match entry {
            IndexEntry::Intent(e) => Some(status_block_for_intent(e)),
            IndexEntry::Assume(_) => None,
        }
    } else {
        None
    };
    match status_block {
        Some(sb) => format!("{header}\n\n{text}\n\n{meta}\n\n{sb}\n\n---\n"),
        None => format!("{header}\n\n{text}\n\n{meta}\n\n---\n"),
    }
}

#[aristo::intent(
    "The `--include-status` block is a blockquote that records the \
     status at MD-generation time. The icon + label are stable; \
     dropping the `(this state is current as of …)` disclaimer would \
     mislead readers into thinking the embedded status is live, which \
     it isn't — it goes stale the moment source code changes. The \
     disclaimer is what keeps the doc artifact honest.",
    verify = "neural",
    id = "doc_include_status_block_records_state_with_staleness_disclaimer"
)]
fn status_block_for_intent(e: &IntentEntry) -> String {
    let icon_state = match e.status {
        Status::Verified => "✓ verified",
        Status::Tested => "✓ tested",
        Status::Neural => "✓ neural",
        Status::Stale => "⚠ stale",
        Status::Orphan => "⚠ orphan",
        Status::Forged => "⚠ forged",
        Status::Counterexample => "✗ counterexample",
        Status::PendingDeepen => "? pending-deepen",
        Status::Unknown => "? unknown",
        Status::Inconclusive => "? inconclusive",
    };
    let commit_hint = match &e.binding {
        BindingState::Certified {
            last_verified_at_commit,
            ..
        } => {
            let hex = last_verified_at_commit.as_str();
            let short: String = hex.chars().take(10).collect();
            format!(" at commit `{short}…`")
        }
        _ => String::new(),
    };
    format!(
        "> **Verification state:** {icon_state}{commit_hint}\n\
         > *(this state is current as of the last `aristo doc --include-status` run; re-run to refresh)*"
    )
}

fn intent_meta_line(id: &AnnotationId, e: &IntentEntry) -> String {
    let mut pieces: Vec<String> = Vec::new();
    pieces.push(format!("Verify level: **{}**", verify_label(e.verify)));
    if matches!(id.namespace(), IdNamespace::Aristos) {
        pieces.push("Server-bound (`aristos:` namespace)".to_string());
    }
    if let Some(parent_text) = parent_link_text(e.parent.as_ref()) {
        pieces.push(parent_text);
    }
    format!("<sub>{}</sub>", pieces.join(" · "))
}

fn assume_meta_line(e: &AssumeEntry) -> String {
    let mut pieces: Vec<String> = vec!["Background fact (no verification target)".to_string()];
    if let Some(parent_text) = parent_link_text(e.parent.as_ref()) {
        pieces.push(parent_text);
    }
    format!("<sub>{}.</sub>", pieces.join(" · "))
}

fn parent_link_text(parent: Option<&ParentLink>) -> Option<String> {
    let parent = parent?;
    let parts: Vec<String> = parent
        .iter()
        .map(|p| format!("[parent: {p}](./{}.md)", id_safe(p)))
        .collect();
    Some(format!("See {}", parts.join(", ")))
}

fn verify_label(level: VerifyLevel) -> &'static str {
    match level {
        VerifyLevel::Bool(false) => "false",
        VerifyLevel::Bool(true) => "true",
        VerifyLevel::Method(VerifyMethod::Neural) => "neural",
        VerifyLevel::Method(VerifyMethod::Test) => "test",
        VerifyLevel::Method(VerifyMethod::Full) => "full",
    }
}

// ─── --summary path ────────────────────────────────────────────────────────

#[aristo::intent(
    "`aristo doc --summary` writes the crate-root `_summary.md` ONLY — \
     it does not also run the per-annotation pass. Combining both is \
     `aristo doc --include-graph` (slice 29). A regression that made \
     `--summary` imply per-annotation writes would surprise users who \
     opted into the cheap summary-only flow for CI gates.",
    verify = "neural",
    id = "doc_summary_writes_summary_only"
)]
fn run_summary(ws: &Workspace, index: &IndexFile) -> CliResult<()> {
    let summary_path = ws.root.join(".aristo").join("doc").join("_summary.md");
    let counts = Counts::from(index);
    let md = render_summary_md(&counts);

    if let Some(parent) = summary_path.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    fs::write(&summary_path, &md).map_err(CliError::Io)?;

    println!();
    println!("→ Reading .aristo/index.toml … ok");
    println!("→ Writing .aristo/doc/_summary.md");
    println!(
        "  • {} annotations ({} intent · {} assume)",
        counts.total, counts.intent, counts.assume,
    );
    println!(
        "  • {} server-bound (aristos: namespace)",
        counts.server_bound,
    );
    println!(
        "  • Verify levels: false={}, neural={}, test={}, full={}",
        counts.verify_false, counts.verify_neural, counts.verify_test, counts.verify_full,
    );
    println!();
    println!("ok: crate-root summary written.");
    println!();
    println!("To render in `cargo doc`, add to your lib.rs / main.rs:");
    println!("    //! ...your existing crate doc...");
    println!("    #![doc = include_str!(\"../.aristo/doc/_summary.md\")]");
    Ok(())
}

#[derive(Debug, Default)]
struct Counts {
    total: usize,
    intent: usize,
    assume: usize,
    server_bound: usize,
    verify_false: usize,
    verify_neural: usize,
    verify_test: usize,
    verify_full: usize,
}

impl Counts {
    fn from(index: &IndexFile) -> Self {
        let mut c = Counts::default();
        for (id, entry) in &index.entries {
            c.total += 1;
            if matches!(id.namespace(), IdNamespace::Aristos) {
                c.server_bound += 1;
            }
            match entry {
                IndexEntry::Intent(e) => {
                    c.intent += 1;
                    c.tally_verify(e);
                }
                IndexEntry::Assume(_) => c.assume += 1,
            }
        }
        c
    }

    fn tally_verify(&mut self, e: &IntentEntry) {
        // `verify = true` resolves to the project default at run time
        // (per slice 22). For the static summary we count it under the
        // default's bucket, defaulting to "test" when unconfigured. This
        // mirrors `aristo status`' verify-level tally policy.
        match e.verify {
            VerifyLevel::Bool(false) => self.verify_false += 1,
            VerifyLevel::Bool(true) => self.verify_test += 1,
            VerifyLevel::Method(VerifyMethod::Neural) => self.verify_neural += 1,
            VerifyLevel::Method(VerifyMethod::Test) => self.verify_test += 1,
            VerifyLevel::Method(VerifyMethod::Full) => self.verify_full += 1,
        }
    }
}

fn render_summary_md(c: &Counts) -> String {
    format!(
        "## Aristo verified annotations\n\
         \n\
         This crate carries **{total} Aristo annotations** ({intent} intent · {assume} assume).\n\
         \n\
         | Verify level | Count |\n\
         |---|---|\n\
         | `false` (documentation only) | {vfalse} |\n\
         | `\"neural\"` | {vneural} |\n\
         | `\"test\"` | {vtest} |\n\
         | `\"full\"` | {vfull} |\n\
         \n\
         **{bound} annotations are server-bound** (`aristos:` namespace) and verified by the\n\
         Aristo proof system. See the [annotation graph](./_graph.svg) for the full\n\
         property structure.\n\
         \n\
         ---\n",
        total = c.total,
        intent = c.intent,
        assume = c.assume,
        vfalse = c.verify_false,
        vneural = c.verify_neural,
        vtest = c.verify_test,
        vfull = c.verify_full,
        bound = c.server_bound,
    )
}

/// Filesystem-safe form of an annotation id: `:` → `__`. Same convention
/// as `.proof` / `.critique` files so the user has one mental model for
/// "how does an id become a filename" across the SDK.
fn id_safe(id: &AnnotationId) -> String {
    id.as_str().replace(':', "__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationId, ArtaId, AssumeEntry, BindingState, CommitHash, CoveredRegion, IndexEntry,
        IndexFile, IntentEntry, Meta, Sha256, Status, VerifiedOutcome,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(verify: VerifyLevel, server_bound: bool) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify,
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: if server_bound {
                BindingState::Certified {
                    linked: ArtaId::parse("arta_op4q3z9NbV").unwrap(),
                    verified_outcome: VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86)))
                        .unwrap(),
                    last_verified_at_commit: CommitHash::parse(&"a".repeat(40)).unwrap(),
                }
            } else {
                BindingState::Local
            },
            parent: None,
        })
    }

    fn assume() -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: "y".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn y (line 2)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn make_index(entries: Vec<(&str, IndexEntry)>) -> IndexFile {
        let mut map = BTreeMap::new();
        for (id, entry) in entries {
            map.insert(AnnotationId::parse(id).unwrap(), entry);
        }
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: map,
        }
    }

    #[test]
    fn counts_tally_intent_assume_and_server_bound() {
        let index = make_index(vec![
            ("local_intent", intent(VerifyLevel::Bool(false), false)),
            (
                "aristos:bound_intent",
                intent(VerifyLevel::Method(VerifyMethod::Full), true),
            ),
            ("an_assume", assume()),
        ]);
        let c = Counts::from(&index);
        assert_eq!(c.total, 3);
        assert_eq!(c.intent, 2);
        assert_eq!(c.assume, 1);
        assert_eq!(c.server_bound, 1);
        assert_eq!(c.verify_false, 1);
        assert_eq!(c.verify_full, 1);
    }

    #[test]
    fn counts_buckets_each_verify_level() {
        let index = make_index(vec![
            ("a", intent(VerifyLevel::Bool(false), false)),
            (
                "b",
                intent(VerifyLevel::Method(VerifyMethod::Neural), false),
            ),
            ("c", intent(VerifyLevel::Method(VerifyMethod::Test), false)),
            ("d", intent(VerifyLevel::Method(VerifyMethod::Full), false)),
            ("e", intent(VerifyLevel::Bool(true), false)),
        ]);
        let c = Counts::from(&index);
        assert_eq!(c.verify_false, 1);
        assert_eq!(c.verify_neural, 1);
        // `true` resolves to the project default at run time, but the
        // static summary counts it under "test" (the free-tier default).
        assert_eq!(c.verify_test, 2);
        assert_eq!(c.verify_full, 1);
    }

    #[test]
    fn render_summary_md_includes_counts_and_table_header() {
        let c = Counts {
            total: 47,
            intent: 33,
            assume: 14,
            server_bound: 20,
            verify_false: 9,
            verify_neural: 4,
            verify_test: 12,
            verify_full: 22,
        };
        let md = render_summary_md(&c);
        assert!(md.contains("**47 Aristo annotations**"));
        assert!(md.contains("(33 intent · 14 assume)"));
        assert!(md.contains("| Verify level | Count |"));
        assert!(md.contains("| `false` (documentation only) | 9 |"));
        assert!(md.contains("**20 annotations are server-bound**"));
    }

    #[test]
    fn id_safe_replaces_colon_with_double_underscore() {
        let id = AnnotationId::parse("aristos:balance_no_duplicate_cells").unwrap();
        assert_eq!(id_safe(&id), "aristos__balance_no_duplicate_cells");
    }

    #[test]
    fn id_safe_leaves_local_id_unchanged() {
        let id = AnnotationId::parse("cells_extracted_without_aliasing").unwrap();
        assert_eq!(id_safe(&id), "cells_extracted_without_aliasing");
    }

    // ─── per-annotation rendering ──────────────────────────────────────

    #[test]
    fn render_intent_uses_aristo_verified_intent_header() {
        let id = AnnotationId::parse("aristos:balance_no_duplicate_cells").unwrap();
        let entry = intent(VerifyLevel::Method(VerifyMethod::Full), true);
        let md = render_annotation_md(&id, &entry, false);
        assert!(
            md.starts_with("**Aristo verified intent — `aristos:balance_no_duplicate_cells`**\n"),
            "got:\n{md}"
        );
    }

    #[test]
    fn render_assume_uses_aristo_assumption_header() {
        let id = AnnotationId::parse("storage_write_atomicity").unwrap();
        let entry = assume();
        let md = render_annotation_md(&id, &entry, false);
        assert!(
            md.starts_with("**Aristo assumption — `storage_write_atomicity`**\n"),
            "got:\n{md}"
        );
    }

    #[test]
    fn render_intent_metadata_marks_server_bound_for_aristos_namespace() {
        let id = AnnotationId::parse("aristos:bound").unwrap();
        let entry = intent(VerifyLevel::Method(VerifyMethod::Full), true);
        let md = render_annotation_md(&id, &entry, false);
        assert!(
            md.contains("Verify level: **full**"),
            "expected verify-level marker; got:\n{md}"
        );
        assert!(
            md.contains("Server-bound (`aristos:` namespace)"),
            "expected server-bound marker; got:\n{md}"
        );
    }

    #[test]
    fn render_intent_metadata_omits_server_bound_for_local() {
        let id = AnnotationId::parse("local_intent").unwrap();
        let entry = intent(VerifyLevel::Bool(false), false);
        let md = render_annotation_md(&id, &entry, false);
        assert!(
            md.contains("Verify level: **false**"),
            "expected verify-level marker; got:\n{md}"
        );
        assert!(
            !md.contains("Server-bound"),
            "did not expect server-bound marker on local entry; got:\n{md}"
        );
    }

    #[test]
    fn render_metadata_includes_parent_link() {
        use aristo_core::index::ParentLink;
        let id = AnnotationId::parse("child").unwrap();
        let mut entry = match intent(VerifyLevel::Method(VerifyMethod::Test), false) {
            IndexEntry::Intent(e) => e,
            _ => unreachable!(),
        };
        entry.parent = Some(ParentLink::Single(
            AnnotationId::parse("balance_no_cells_lost").unwrap(),
        ));
        let md = render_annotation_md(&id, &IndexEntry::Intent(entry), false);
        assert!(
            md.contains("See [parent: balance_no_cells_lost](./balance_no_cells_lost.md)"),
            "expected parent link; got:\n{md}"
        );
    }

    #[test]
    fn render_ends_with_separator_rule() {
        let id = AnnotationId::parse("any").unwrap();
        let entry = intent(VerifyLevel::Bool(false), false);
        let md = render_annotation_md(&id, &entry, false);
        assert!(
            md.trim_end().ends_with("\n---"),
            "expected trailing `---` separator; got:\n{md}"
        );
    }

    // ─── --include-status ──────────────────────────────────────────────

    #[test]
    fn render_intent_with_include_status_appends_blockquote() {
        let id = AnnotationId::parse("aristos:bound").unwrap();
        let entry = intent(VerifyLevel::Method(VerifyMethod::Full), true);
        // Force status to verified for this test.
        let entry = match entry {
            IndexEntry::Intent(mut e) => {
                e.status = Status::Verified;
                IndexEntry::Intent(e)
            }
            other => other,
        };
        let md = render_annotation_md(&id, &entry, true);
        assert!(
            md.contains("> **Verification state:** ✓ verified"),
            "expected verified blockquote; got:\n{md}"
        );
        assert!(
            md.contains("state is current as of the last"),
            "expected staleness disclaimer; got:\n{md}"
        );
    }

    #[test]
    fn render_intent_with_include_status_includes_commit_hint_for_certified() {
        let id = AnnotationId::parse("aristos:bound").unwrap();
        let entry = intent(VerifyLevel::Method(VerifyMethod::Full), true);
        let md = render_annotation_md(&id, &entry, true);
        // CommitHash in the fixture is "a" * 40; first 10 chars = "aaaaaaaaaa".
        assert!(
            md.contains("at commit `aaaaaaaaaa…`"),
            "expected 10-char commit hint; got:\n{md}"
        );
    }

    #[test]
    fn render_intent_with_include_status_omits_commit_hint_for_local() {
        let id = AnnotationId::parse("local").unwrap();
        let entry = intent(VerifyLevel::Bool(false), false);
        let md = render_annotation_md(&id, &entry, true);
        assert!(
            !md.contains("at commit"),
            "did not expect commit hint on local entry; got:\n{md}"
        );
    }

    #[test]
    fn render_assume_with_include_status_omits_blockquote() {
        // Assumes are not verification targets; status block is skipped
        // even when --include-status is set.
        let id = AnnotationId::parse("storage_write_atomicity").unwrap();
        let entry = assume();
        let md = render_annotation_md(&id, &entry, true);
        assert!(
            !md.contains("Verification state"),
            "did not expect blockquote on assume; got:\n{md}"
        );
    }

    #[test]
    fn status_block_uses_warn_icon_for_stale() {
        let mut e = match intent(VerifyLevel::Method(VerifyMethod::Full), true) {
            IndexEntry::Intent(e) => e,
            _ => unreachable!(),
        };
        e.status = Status::Stale;
        let block = status_block_for_intent(&e);
        assert!(block.contains("⚠ stale"), "got: {block}");
    }

    #[test]
    fn status_block_uses_cross_icon_for_counterexample() {
        let mut e = match intent(VerifyLevel::Method(VerifyMethod::Full), true) {
            IndexEntry::Intent(e) => e,
            _ => unreachable!(),
        };
        e.status = Status::Counterexample;
        let block = status_block_for_intent(&e);
        assert!(block.contains("✗ counterexample"), "got: {block}");
    }

    // ─── file_unchanged ────────────────────────────────────────────────

    #[test]
    fn file_unchanged_returns_true_for_byte_equal_disk_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "hello\n").unwrap();
        assert!(file_unchanged(tmp.path(), "hello\n"));
    }

    #[test]
    fn file_unchanged_returns_false_for_drift() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "before\n").unwrap();
        assert!(!file_unchanged(tmp.path(), "after\n"));
    }

    #[test]
    fn file_unchanged_returns_false_for_missing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let missing = tmp.path().join("nope.md");
        assert!(!file_unchanged(&missing, "anything"));
    }
}
