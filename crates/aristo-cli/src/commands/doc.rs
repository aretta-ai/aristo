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

use aristo_core::index::{
    AnnotationId, IdNamespace, IndexEntry, IndexFile, IntentEntry, VerifyLevel, VerifyMethod,
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

    // Per-annotation rendering + --include-status + --check land in
    // follow-up slice-28 commits. Return NotImplemented with the slice
    // pointer so `binary_smoke::defined_but_unimplemented_subcommand_exits_64`
    // and any user invocation get a clear "coming in slice 28" message.
    let _ = include_status;
    let _ = check;
    Err(CliError::NotImplemented {
        what: "aristo doc (per-annotation markdown)",
        slice: "slice 28",
    })
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
#[allow(dead_code)]
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
}
